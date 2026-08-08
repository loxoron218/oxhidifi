//! Album cover cards with async artwork loading.

use std::{boxed::Box, sync::Arc};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gdk::MemoryTexture,
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::{End, Start},
            Box as GtkBox,
            ContentFit::Cover,
            EventControllerMotion, GestureClick, Image, Label,
            Orientation::{Horizontal, Vertical},
            Overlay, Picture, Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::{AppState, NavigationEvent::AlbumDetail},
    storage::{formats::FormatInfo, records::Album},
    ui::{
        ArtworkDecodeRequest, CoverArtCache, DecodedCover, build_album_play_button,
        library::album_playback::{album_play_icon, toggle_or_play_album},
        raw_to_texture,
    },
    zoom::grid_cover_size,
};

/// Build a placeholder cover art widget.
///
/// Returns an `Image` with a generic audio icon. Used as the initial
/// state before async cover art loading completes.
#[must_use]
pub fn build_placeholder(size: i32) -> Widget {
    let placeholder = Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(size / 2)
        .width_request(size)
        .height_request(size)
        .css_classes(["album-cover", "dim-label"])
        .build();
    placeholder.update_property(&[PropertyLabel("Album cover placeholder")]);
    placeholder.upcast()
}

/// Apply a decoded texture to an overlay's child.
///
/// If the child is already a `Picture`, updates its paintable in place.
/// Otherwise replaces the child with a new `Picture`.
fn apply_texture(overlay: &Overlay, texture: &MemoryTexture, size: i32) {
    let updated = overlay.child().and_then(|c| {
        c.downcast_ref::<Picture>()
            .map(|p| p.set_paintable(Some(texture)))
    });
    if updated.is_none() {
        let picture = Picture::builder()
            .paintable(texture)
            .content_fit(Cover)
            .width_request(size)
            .height_request(size)
            .css_classes(["album-cover"])
            .build();
        picture.update_property(&[PropertyLabel("Album cover art")]);
        overlay.set_child(Some(&picture));
    }
}

/// Resolve a single card's cover widget to `size`.
///
/// Applies a cached texture for the size immediately, or swaps a decoded
/// `Picture` for a placeholder when the size is not yet cached, so a card
/// never shows a texture decoded at a stale size. Does not dispatch new
/// decode requests — the caller defers those to the debounced path.
pub fn resolve_cover_widget(cache: &CoverArtCache, overlay: &Overlay, album_id: i64, size: i32) {
    if let Some(texture) = cache.get(album_id, size) {
        apply_texture(overlay, &texture, size);
        return;
    }
    if overlay
        .child()
        .is_some_and(|child| child.downcast_ref::<Picture>().is_some())
    {
        overlay.set_child(Some(&build_placeholder(size)));
    }
}

/// Send decoded album cover through the channel, logging on failure.
fn try_send_album_cover(
    tx: &Sender<(usize, i64, DecodedCover)>,
    index: usize,
    album_id: i64,
    decoded: Option<DecodedCover>,
) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send((index, album_id, decoded)) {
        error!(error = %e, "Failed to send decoded album cover to main thread");
    }
}

/// Check the shared [`CoverArtCache`] and dispatch decode requests for
/// missing covers to the centralized decoder.
///
/// Each decoded cover is written to the cache and applied to its overlay via a
/// [`spawn_future_local`] async task that stays alive until all results
/// are received, preventing a race where the channel receiver is dropped
/// before the background decoder finishes.
pub fn load_cover_art_async(
    state: &Arc<AppState>,
    cover_art_data: &[(i64, usize, String)],
    overlays: &[Overlay],
    cache: &Arc<CoverArtCache>,
    size: i32,
) {
    if cover_art_data.is_empty() {
        return;
    }

    let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
    let mut uncached: Vec<(i64, usize, String)> = Vec::new();

    for (album_id, index, path) in cover_art_data {
        let Some(overlay) = overlays.get(*index) else {
            continue;
        };
        if let Some(texture) = cache.get(*album_id, size) {
            apply_texture(overlay, &texture, size);
            continue;
        }
        uncached.push((*album_id, *index, path.clone()));
    }

    if uncached.is_empty() {
        return;
    }

    for (album_id, index, path) in uncached {
        let tx = tx.clone();
        cache.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size,
            on_complete: Box::new(move |_, decoded| {
                try_send_album_cover(&tx, index, album_id, decoded);
            }),
        });
    }
    drop(tx);

    let overlays: Vec<Overlay> = overlays.to_vec();
    let cache_clone = Arc::clone(cache);
    let state = Arc::clone(state);

    spawn_future_local(async move {
        while let Ok((index, album_id, decoded)) = rx.recv().await {
            apply_decoded_cover_if_current(
                &state,
                &cache_clone,
                &overlays,
                index,
                album_id,
                &decoded,
                size,
            );
        }
    });
}

/// Insert a decoded cover into the cache and apply it to its card, skipping
/// sizes superseded by a newer zoom *before* allocating a texture, so stale
/// decodes never do main-thread texture work or churn the cache.
fn apply_decoded_cover_if_current(
    state: &Arc<AppState>,
    cache: &CoverArtCache,
    overlays: &[Overlay],
    index: usize,
    album_id: i64,
    decoded: &DecodedCover,
    size: i32,
) {
    if size != grid_cover_size(state.storage.get_grid_zoom_level()) {
        return;
    }
    let texture = raw_to_texture(decoded);
    cache.insert(album_id, size, texture.clone());
    apply_decoded_cover(state, overlays, index, &texture, size);
}

/// Apply a decoded cover to its card, skipping sizes superseded by a newer
/// zoom. The caller already skipped stale sizes before creating the texture
/// (see `load_cover_art_async`); this guards against a size that became
/// stale between the guard and the widget apply.
fn apply_decoded_cover(
    state: &Arc<AppState>,
    overlays: &[Overlay],
    index: usize,
    texture: &MemoryTexture,
    size: i32,
) {
    if size == grid_cover_size(state.storage.get_grid_zoom_level())
        && let Some(overlay) = overlays.get(index)
    {
        apply_texture(overlay, texture, size);
    }
}

/// Build the cover art overlay with hover play button for an album card.
fn build_card_overlay(state: &Arc<AppState>, album_id: i64, size: i32) -> Overlay {
    let cover_art = build_placeholder(size);

    let overlay = Overlay::new();
    overlay.set_child(Some(&cover_art));
    overlay.set_css_classes(&["cover-overlay"]);

    let play_button = build_album_play_button();
    play_button.set_visible(false);

    overlay.add_overlay(&play_button);

    let motion_ctrl = EventControllerMotion::new();
    let btn_show = play_button.clone();
    let state_enter = Arc::clone(state);
    motion_ctrl.connect_enter(move |_, _, _| {
        btn_show.set_icon_name(album_play_icon(&state_enter, album_id));
        btn_show.set_visible(true);
    });
    let btn_hide = play_button.clone();
    motion_ctrl.connect_leave(move |_| {
        btn_hide.set_visible(false);
    });
    overlay.add_controller(motion_ctrl);

    let state_clone = Arc::clone(state);
    let btn_click = play_button.clone();
    play_button.connect_clicked(move |_| {
        let icon = album_play_icon(&state_clone, album_id);
        btn_click.set_icon_name(if icon == "media-playback-pause-symbolic" {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });

        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            toggle_or_play_album(&state, album_id).await;
        });
    });

    overlay
}

/// Build a single album card widget.
///
/// Returns a `Box` containing a vertical layout with cover art,
/// title, artist, format summary, and year labels. Uses
/// `GestureClick` for click handling instead of `Button` to avoid
/// theme-inflated natural sizing from the `card` CSS class.
///
/// Also returns the `Overlay` wrapping the cover art so it can be
/// resized in place on zoom changes and updated asynchronously after
/// the card is added to the container.
///
/// # Arguments
///
/// * `state` - Application state
/// * `album` - Album data
/// * `artist_name` - Display name of the album artist
/// * `format_info` - Format summary for the album
/// * `size` - Cover art size in pixels (derived from the grid zoom level)
#[must_use]
pub fn build_album_card(
    state: &Arc<AppState>,
    album: &Album,
    artist_name: &str,
    format_info: &FormatInfo,
    size: i32,
) -> (GtkBox, Overlay) {
    let card = GtkBox::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .tooltip_text(format!(
            "Play \u{201c}{}\u{201d} by album artist",
            album.title
        ))
        .build();
    card.update_property(&[PropertyLabel(&format!(
        "Play \u{201c}{}\u{201d} by album artist",
        album.title
    ))]);

    let album_id = album.id;

    let overlay = build_card_overlay(state, album_id, size);

    card.append(&overlay.clone().upcast::<Widget>());

    let title_label = Label::builder()
        .label(&album.title)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel(&format!("Album: {}", album.title))]);

    let artist_label = Label::builder()
        .label(artist_name)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    artist_label.update_property(&[PropertyLabel(&format!("Artist: {artist_name}"))]);

    let format_row = GtkBox::builder().orientation(Horizontal).spacing(6).build();

    let format_label = Label::builder()
        .label(format_info.summary())
        .ellipsize(EllipsizeEnd)
        .max_width_chars(14)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    format_label.update_property(&[PropertyLabel(&format!("Format: {}", format_info.summary()))]);
    format_label.set_hexpand(true);

    let year_label = Label::builder()
        .label(album.year.map_or(String::new(), |y| y.to_string()))
        .css_classes(["dim-label", "caption"])
        .halign(End)
        .build();
    year_label.update_property(&[PropertyLabel("Release year")]);

    format_row.append(&format_label);
    format_row.append(&year_label);

    card.append(&title_label);
    card.append(&artist_label);
    card.append(&format_row);

    if !state.storage.get_show_album_labels() {
        title_label.set_visible(false);
        artist_label.set_visible(false);
        format_row.set_visible(false);
    }

    let gesture = GestureClick::new();
    let state_clone = Arc::clone(state);
    gesture.connect_released(move |_, _, _, _| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            state.send_navigation_event(AlbumDetail(album_id)).await;
        });
    });
    card.add_controller(gesture);

    (card, overlay)
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        async_channel::unbounded,
        libadwaita::gtk::{self, test},
    };

    use crate::ui::{
        DecodedCover, library::album_card::try_send_album_cover, tests::mock_decoded_cover,
    };

    #[test]
    fn try_send_album_cover_none_is_noop() -> Result<()> {
        let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
        try_send_album_cover(&tx, 0, 1, None);
        ensure!(rx.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn try_send_album_cover_forwards_decoded() -> Result<()> {
        let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
        try_send_album_cover(&tx, 3, 9, Some(mock_decoded_cover()));
        ensure!(matches!(rx.try_recv(), Ok((3, 9, _))));
        Ok(())
    }
}

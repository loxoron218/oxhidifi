//! Album cover cards with async artwork loading.

use std::sync::Arc;

use libadwaita::{
    glib::{prelude::Cast, spawn_future_local},
    gtk::{
        Align::{End, Start},
        Box as GtkBox, EventControllerMotion, GestureClick, Image, Label,
        Orientation::{Horizontal, Vertical},
        Overlay, Widget,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End as EllipsizeEnd,
    },
    prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::AlbumDetail},
    storage::{catalog::Album, formats::FormatInfo},
    ui::{
        gallery::play_action::{album_play_icon, toggle_or_play_album},
        osd_button::build_album_play_button,
    },
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

/// Build the cover art overlay with hover play button for an album card.
fn build_card_overlay(state: &Arc<AppState>, album_id: i64, size: i32) -> Overlay {
    let cover_art = build_placeholder(size);

    let overlay = Overlay::builder()
        .width_request(size)
        .height_request(size)
        .halign(Start)
        .build();
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

/// Calculate the title label's character limit for a cover size.
fn title_max_chars(size: i32) -> i32 {
    (size / 10).max(6)
}

/// Calculate the artist label's character limit for a cover size.
fn artist_max_chars(size: i32) -> i32 {
    (size / 8).max(8)
}

/// Calculate the format label's character limit for a cover size.
fn format_max_chars(size: i32) -> i32 {
    (size / 10).max(6)
}

/// Resize an album card's layout and metadata constraints to `size`.
pub fn resize_album_card(overlay: &Overlay, size: i32) {
    let Some(parent) = overlay.parent() else {
        return;
    };
    let Ok(card) = parent.downcast::<GtkBox>() else {
        return;
    };
    card.set_width_request(size);

    let Some(next) = overlay.next_sibling() else {
        return;
    };
    let Ok(title) = next.downcast::<Label>() else {
        return;
    };
    title.set_max_width_chars(title_max_chars(size));

    let Some(next) = title.next_sibling() else {
        return;
    };
    let Ok(artist) = next.downcast::<Label>() else {
        return;
    };
    artist.set_max_width_chars(artist_max_chars(size));

    let Some(next) = artist.next_sibling() else {
        return;
    };
    let Ok(format_row) = next.downcast::<GtkBox>() else {
        return;
    };
    format_row.set_width_request(size);
    if let Some(child) = format_row.first_child()
        && let Ok(format_label) = child.downcast::<Label>()
    {
        format_label.set_max_width_chars(format_max_chars(size));
    }
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
        .width_request(size)
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
        .max_width_chars(title_max_chars(size))
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel(&format!("Album: {}", album.title))]);

    let artist_label = Label::builder()
        .label(artist_name)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(artist_max_chars(size))
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    artist_label.update_property(&[PropertyLabel(&format!("Artist: {artist_name}"))]);

    let format_row = GtkBox::builder()
        .orientation(Horizontal)
        .spacing(6)
        .width_request(size)
        .build();

    let (format_text, format_tooltip) = if format_info.is_mixed() {
        ("Mixed".to_string(), format_info.summary())
    } else {
        let s = format_info.summary();
        (s.clone(), s)
    };

    let format_label = Label::builder()
        .label(&format_text)
        .tooltip_text(&format_tooltip)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(format_max_chars(size))
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .hexpand(true)
        .build();
    format_label.update_property(&[PropertyLabel(&format!("Format: {format_tooltip}"))]);

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
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            glib::prelude::Cast,
            gtk::{self, Box, Label, Overlay, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::{catalog::Album, formats::FormatInfo},
        ui::gallery::card::{
            artist_max_chars, build_album_card, format_max_chars, resize_album_card,
            title_max_chars,
        },
    };

    fn build_mock_card(size: i32) -> Result<(Box, Overlay)> {
        let album = Album {
            id: 7,
            title: "Test Album".into(),
            artist_id: 1,
            year: Some(2024),
            genre: None,
            artwork_path: None,
            track_count: 12,
            total_duration: 3600.0,
            format_summary: "FLAC".into(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96000),
        };
        let state = Arc::new(AppState::mock()?);
        Ok(build_album_card(
            &state,
            &album,
            "Test Artist",
            &FormatInfo::default(),
            size,
        ))
    }

    #[test]
    fn max_chars_scale_with_cover_size() {
        let cases = [
            (0, 6, 8),
            (40, 6, 8),
            (60, 6, 8),
            (72, 7, 9),
            (100, 10, 12),
            (120, 12, 15),
            (180, 18, 22),
            (240, 24, 30),
        ];
        for (size, title, artist) in cases {
            assert_eq!(
                title_max_chars(size),
                title,
                "cover size {size} px must cap the title at {title} chars"
            );
            assert_eq!(
                format_max_chars(size),
                title,
                "cover size {size} px must cap the format at {title} chars"
            );
            assert_eq!(
                artist_max_chars(size),
                artist,
                "cover size {size} px must cap the artist at {artist} chars"
            );
        }
    }

    #[test]
    fn resize_album_card_scales_layout_and_labels() -> Result<()> {
        let (card, overlay) = build_mock_card(120)?;
        for size in [120, 200, 40] {
            resize_album_card(&overlay, size);
            ensure!(
                card.width_request() == size,
                "card width must follow the cover size"
            );
            let title = overlay.next_sibling();
            let Some(title) = title.as_ref().and_then(|w| w.downcast_ref::<Label>()) else {
                bail!("title label must follow the overlay in the card");
            };
            let cap = title_max_chars(size);
            ensure!(
                title.max_width_chars() == cap,
                "title must cap at {cap} chars for cover size {size}"
            );
            let artist = title.next_sibling();
            let Some(artist) = artist.as_ref().and_then(|w| w.downcast_ref::<Label>()) else {
                bail!("artist label must follow the title label");
            };
            let cap = artist_max_chars(size);
            ensure!(
                artist.max_width_chars() == cap,
                "artist must cap at {cap} chars for cover size {size}"
            );
            let format_row = artist.next_sibling();
            let Some(format_row) = format_row.as_ref().and_then(|w| w.downcast_ref::<Box>()) else {
                bail!("format row must follow the artist label");
            };
            ensure!(
                format_row.width_request() == size,
                "format row width must follow the cover size"
            );
            let format_label = format_row.first_child();
            let Some(format_label) = format_label
                .as_ref()
                .and_then(|w| w.downcast_ref::<Label>())
            else {
                bail!("format row must start with the format label");
            };
            let cap = format_max_chars(size);
            ensure!(
                format_label.max_width_chars() == cap,
                "format label must cap at {cap} chars for cover size {size}"
            );
        }
        Ok(())
    }
}

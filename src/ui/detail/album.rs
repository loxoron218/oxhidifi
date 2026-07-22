//! Album detail page with artwork, metadata, and track listing.

use std::{boxed::Box, sync::Arc};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            MainContext, idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{
            Align::Start,
            Box as GtkBox, Button,
            ContentFit::Cover,
            Label, ListBox,
            Orientation::Horizontal,
            Overlay, Picture, ScrolledWindow, Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
            prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::{AppState, NavigationEvent},
    playback::control::PlaybackController,
    storage::{Storage, Track},
    ui::{
        ArtworkDecodeRequest, DecodedCover, build_album_play_button,
        detail::common::{build_detail_wrapper, build_scroll_content, fill_track_list_batch},
        library::albums::{album_play_icon, toggle_or_play_album},
        raw_to_texture,
    },
};

/// Size of the album cover artwork on the detail page in pixels.
const DETAIL_COVER_SIZE: i32 = 320;

/// Owned widgets for the album detail content area.
struct AlbumDetailContent {
    /// The scroll window wrapping content.
    scroll: ScrolledWindow,
    /// Album artwork display.
    artwork: Picture,
    /// Play/pause button overlaid on the artwork.
    play_button: Button,
    /// Album title label.
    title_label: Label,
    /// Artist name label.
    artist_label: Label,
    /// Release year label.
    year_label: Label,
    /// Genre label.
    genre_label: Label,
    /// Format summary label.
    format_label: Label,
    /// Track listing container.
    track_list: ListBox,
}

/// Widget references for the album detail page.
struct AlbumDetailWidgets<'a> {
    /// Album artwork display.
    artwork: &'a Picture,
    /// Album title label.
    title_label: &'a Label,
    /// Artist name label.
    artist_label: &'a Label,
    /// Release year label.
    year_label: &'a Label,
    /// Genre label.
    genre_label: &'a Label,
    /// Format summary label (sample rate, bit depth, etc.).
    format_label: &'a Label,
    /// Track listing container.
    track_list: &'a ListBox,
}

/// Build the scrollable content area with album widgets.
fn build_album_content() -> AlbumDetailContent {
    let (scroll, content) = build_scroll_content();

    let artwork = Picture::builder()
        .content_fit(Cover)
        .can_shrink(true)
        .css_classes(["album-cover"])
        .build();
    artwork.update_property(&[PropertyLabel("Album artwork")]);

    let artwork_wrapper = GtkBox::builder()
        .orientation(Horizontal)
        .width_request(DETAIL_COVER_SIZE)
        .height_request(DETAIL_COVER_SIZE)
        .halign(Start)
        .build();
    artwork_wrapper.append(&artwork);

    let overlay = Overlay::new();
    overlay.set_child(Some(&artwork_wrapper));
    overlay.set_css_classes(&["cover-overlay"]);
    overlay.set_halign(Start);

    let play_button = build_album_play_button();
    overlay.add_overlay(&play_button);
    content.append(&overlay);

    let title_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(End)
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel("Album title")]);
    content.append(&title_label);

    let artist_label = Label::builder()
        .css_classes(["title-4", "accent"])
        .ellipsize(End)
        .halign(Start)
        .build();
    artist_label.update_property(&[PropertyLabel("Artist name")]);
    content.append(&artist_label);

    let meta_box = GtkBox::builder()
        .orientation(Horizontal)
        .spacing(12)
        .halign(Start)
        .build();

    let year_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    year_label.update_property(&[PropertyLabel("Release year")]);
    meta_box.append(&year_label);

    let genre_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    genre_label.update_property(&[PropertyLabel("Genre")]);
    meta_box.append(&genre_label);

    let format_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    format_label.update_property(&[PropertyLabel("Audio format")]);
    meta_box.append(&format_label);

    content.append(&meta_box);

    let tracks_header = Label::builder()
        .label("Tracks")
        .css_classes(["title-4", "heading"])
        .halign(Start)
        .build();
    tracks_header.update_property(&[PropertyLabel("Track list")]);
    content.append(&tracks_header);

    let track_list = ListBox::builder().css_classes(["boxed-list"]).build();
    content.append(&track_list);

    scroll.set_child(Some(&content));

    AlbumDetailContent {
        scroll,
        artwork,
        play_button,
        title_label,
        artist_label,
        year_label,
        genre_label,
        format_label,
        track_list,
    }
}

/// Build the album detail page widget.
#[must_use]
pub fn build_album_detail(
    state: &Arc<AppState>,
    album_id: i64,
    nav_tx: &Sender<NavigationEvent>,
) -> Widget {
    let wrapper = build_detail_wrapper(nav_tx, "Album");

    let content = build_album_content();
    wrapper.append(&content.scroll);

    content
        .play_button
        .set_icon_name(album_play_icon(state, album_id));
    let click_state = Arc::clone(state);
    let click_aid = album_id;
    let click_btn = content.play_button.clone();
    content.play_button.connect_clicked(move |_| {
        let icon = album_play_icon(&click_state, click_aid);
        click_btn.set_icon_name(if icon == "media-playback-pause-symbolic" {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });

        let s = Arc::clone(&click_state);
        let aid = click_aid;
        spawn_future_local(async move {
            toggle_or_play_album(&s, aid).await;
        });
    });

    let ev_rx = state.playback.subscribe();
    let ev_btn = content.play_button.clone();
    let ev_state = Arc::clone(state);
    let ev_aid = album_id;
    MainContext::default().spawn_local(async move {
        while let Ok(_event) = ev_rx.recv().await {
            let icon = album_play_icon(&ev_state, ev_aid);
            let btn = ev_btn.clone();
            idle_add_local(move || update_detail_play_button(&btn, icon));
        }
    });

    let sc = Arc::clone(state);
    spawn_future_local(async move {
        populate_album_detail(
            &sc,
            album_id,
            AlbumDetailWidgets {
                artwork: &content.artwork,
                title_label: &content.title_label,
                artist_label: &content.artist_label,
                year_label: &content.year_label,
                genre_label: &content.genre_label,
                format_label: &content.format_label,
                track_list: &content.track_list,
            },
        )
        .await;
    });

    wrapper.upcast()
}

/// Update the detail page play button icon via idle callback.
fn update_detail_play_button(btn: &Button, icon: &'static str) -> ControlFlow {
    btn.set_icon_name(icon);
    Break
}

/// Try to send decoded cover to the main thread channel, logging on failure.
fn try_send_cover(tx: &Sender<DecodedCover>, decoded: Option<DecodedCover>) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send(decoded) {
        error!(error = %e, "Failed to send decoded album detail cover to main thread");
    }
}

/// Poll for decoded artwork and apply it to the picture widget.
fn poll_artwork(rx: &Receiver<DecodedCover>, artwork: &Picture) -> ControlFlow {
    rx.try_recv().map_or(Continue, |decoded| {
        let texture = raw_to_texture(&decoded);
        artwork.set_paintable(Some(&texture));
        Break
    })
}

/// Load album data from storage and populate the detail UI elements.
async fn populate_album_detail(
    state: &Arc<AppState>,
    album_id: i64,
    widgets: AlbumDetailWidgets<'_>,
) {
    let album = match state.storage.get_album(album_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(album_id, "Album not found");
            return;
        }
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album");
            return;
        }
    };

    if let Some(path) = &album.artwork_path {
        let path = path.clone();
        let (tx, rx) = unbounded::<DecodedCover>();

        state.cover_art_cache.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size: DETAIL_COVER_SIZE,
            on_complete: Box::new(move |_, decoded| try_send_cover(&tx, decoded)),
        });

        let artwork = widgets.artwork.clone();
        idle_add_local(move || poll_artwork(&rx, &artwork));
    }

    widgets.title_label.set_label(&album.title);

    let artist_name = match state.storage.get_artist(album.artist_id).await {
        Ok(Some(a)) => a.name,
        _ => "Unknown Artist".to_string(),
    };
    widgets.artist_label.set_label(&artist_name);

    if let Some(year) = album.year {
        widgets.year_label.set_label(&year.to_string());
        widgets.year_label.set_visible(true);
    } else {
        widgets.year_label.set_visible(false);
    }

    if let Some(genre) = &album.genre {
        widgets.genre_label.set_label(genre);
        widgets.genre_label.set_visible(true);
    } else {
        widgets.genre_label.set_visible(false);
    }

    let format_info = state
        .storage
        .get_album_format_info(album_id)
        .await
        .unwrap_or_default();
    widgets.format_label.set_label(&format!(
        "{} {}\u{2022} {}",
        album.track_count,
        if album.track_count == 1 {
            "track "
        } else {
            "tracks "
        },
        format_info.summary_detailed(),
    ));

    let tracks = match state.storage.get_tracks_by_album(album_id).await {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album tracks");
            return;
        }
    };

    let track_list = widgets.track_list.clone();
    let mut remaining: Vec<(Track, usize)> = tracks
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i + 1))
        .collect::<Vec<_>>();
    remaining.reverse();

    let state = Arc::clone(state);
    idle_add_local(move || fill_track_list_batch(&mut remaining, &track_list, &state));
}

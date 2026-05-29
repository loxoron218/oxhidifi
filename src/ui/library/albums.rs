//! Album grid view with cover art thumbnails.
//!
//! Displays albums in a responsive `FlowBox` grid. Each album cell shows
//! cover art (or a placeholder icon), title, and artist name. Clicking an
//! album triggers playback via the `PlaybackController`.

use std::sync::Arc;

use {
    libadwaita::{
        glib::spawn_future_local,
        gtk::{
            Align::{Center, Start},
            Box, Button,
            ContentFit::Cover,
            FlowBox, Image, Label,
            Orientation::Vertical,
            Picture, ScrolledWindow,
            SelectionMode::None,
            Widget,
            pango::EllipsizeMode::End,
            prelude::*,
        },
    },
    tracing::info,
};

use crate::{
    app::AppState,
    playback::engine::PlaybackController,
    storage::{Album, Storage},
};

/// Size of album cover art thumbnails in pixels.
const THUMBNAIL_SIZE: i32 = 180;

/// Build the album grid view.
///
/// Creates a `ScrolledWindow` containing a `FlowBox` populated with
/// album cards loaded asynchronously from storage.
#[must_use]
pub fn build_album_grid(state: &Arc<AppState>) -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let flow_box = FlowBox::builder()
        .valign(Start)
        .halign(Center)
        .row_spacing(12)
        .column_spacing(12)
        .selection_mode(None)
        .can_focus(true)
        .tooltip_text("Album library grid \u{2014} click an album to play")
        .build();

    let state_clone = Arc::clone(state);
    let flow_ref = flow_box.clone();
    spawn_future_local(async move {
        load_albums(&state_clone, &flow_ref).await;
    });

    scrolled.set_child(Some(&flow_box));
    scrolled
}

/// Load albums from storage and populate the flow box.
///
/// Each album is wrapped in a `Button` so clicking it triggers playback
/// of the album's tracks.
async fn load_albums(state: &Arc<AppState>, flow_box: &FlowBox) {
    let albums = match state.storage.get_all_albums().await {
        Ok(a) => a,
        Err(e) => {
            info!(error = %e, "Failed to load albums");
            return;
        }
    };

    for album in &albums {
        let card = build_album_card(state, album);
        flow_box.append(&card);
    }
}

/// Build the cover art widget for an album.
///
/// Returns either a `Picture` with the album artwork loaded from disk,
/// or a placeholder `Image` with a generic audio icon.
fn build_cover_art(album: &Album) -> Widget {
    album.artwork_path.as_ref().map_or_else(
        || {
            let placeholder = Image::builder()
                .icon_name("audio-x-generic-symbolic")
                .pixel_size(THUMBNAIL_SIZE / 2)
                .width_request(THUMBNAIL_SIZE)
                .height_request(THUMBNAIL_SIZE)
                .css_classes(["album-cover", "dim-label"])
                .build();
            placeholder.upcast()
        },
        |path| {
            let picture = Picture::builder()
                .width_request(THUMBNAIL_SIZE)
                .height_request(THUMBNAIL_SIZE)
                .content_fit(Cover)
                .css_classes(["album-cover"])
                .build();
            picture.set_filename(Some(path));
            picture.upcast()
        },
    )
}

/// Build a single album card widget.
///
/// Returns a `Button` containing a vertical layout with cover art and
/// title/artist labels.
fn build_album_card(state: &Arc<AppState>, album: &Album) -> Button {
    let card = Button::builder()
        .css_classes(["flat", "card"])
        .can_focus(true)
        .tooltip_text(format!(
            "Play \u{201c}{}\u{201d} by album artist",
            album.title
        ))
        .build();

    let content = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .halign(Center)
        .build();

    let cover_art = build_cover_art(album);
    content.append(&cover_art);

    let title_label = Label::builder()
        .label(&album.title)
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .build();

    let track_count_label = Label::builder()
        .label(format!("{} tracks", album.track_count))
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .build();

    content.append(&title_label);
    content.append(&track_count_label);

    card.set_child(Some(&content));

    let state_clone = Arc::clone(state);
    let album_id = album.id;
    card.connect_clicked(move |_| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            play_album(&state, album_id).await;
        });
    });

    card
}

/// Play all tracks in an album by queueing them and starting playback.
///
/// Fetches tracks ordered by track number, queues them, and calls
/// `play_queue` on the playback controller.
async fn play_album(state: &Arc<AppState>, album_id: i64) {
    let tracks = match state.storage.get_tracks_by_album(album_id).await {
        Ok(t) => t,
        Err(e) => {
            info!(error = %e, album_id, "Failed to fetch album tracks");
            return;
        }
    };

    if tracks.is_empty() {
        info!(album_id, "Album has no tracks");
        return;
    }

    let track_ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();

    if let Err(e) = state.playback.play_queue(track_ids) {
        info!(error = %e, album_id, "Failed to start album playback");
    }
}

//! Album grid view with cover art thumbnails.
//!
//! Displays albums in a responsive `FlowBox` grid. Each album cell shows
//! cover art (or a placeholder icon), title, and artist name. Clicking an
//! album triggers playback via the `PlaybackController`.
//! Shows an inline empty state when no albums are available.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use {
    libadwaita::{
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::{Center, End, Start},
            Box, Button,
            ContentFit::Cover,
            EventControllerMotion, Image, Label,
            Orientation::{Horizontal, Vertical},
            Overlay, Picture, ScrolledWindow, Widget,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{BoxExt, ButtonExt, WidgetExt},
    },
    tracing::info,
};

use crate::{
    app::AppState,
    playback::{
        OutputError::{DeviceDisconnected, NoDeviceAvailable},
        PlaybackError::{
            DeviceDisconnected as PlaybackDeviceDisconnected,
            NoDeviceAvailable as PlaybackNoDeviceAvailable, Output,
        },
        engine::PlaybackController,
    },
    storage::{
        Album, Storage,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::library::empty::{
        EmptyStateParams, build_empty_state, build_library_grid, populate_grid, populate_list,
    },
};

/// Size of album cover art thumbnails in pixels.
const THUMBNAIL_SIZE: i32 = 180;

/// Build the album grid view.
///
/// Creates a `ScrolledWindow` containing a `FlowBox` populated with
/// album cards loaded asynchronously from storage. Shows an inline
/// empty state when no albums are available.
#[must_use]
pub fn build_album_grid(state: &Arc<AppState>) -> ScrolledWindow {
    build_library_grid(
        state,
        "Album library grid \u{2014} click an album to play",
        |state, container, scrolled, mode| {
            spawn_future_local(async move {
                load_albums(&state, &container, &scrolled, mode).await;
            });
        },
    )
}

/// Load albums from storage and populate the container.
///
/// If no albums exist, shows an inline empty state with an "Add Folder" button.
/// Populates either a grid (`FlowBox`) or column (`ListBox`) depending on view mode.
async fn load_albums(
    state: &Arc<AppState>,
    container: &Box,
    scrolled: &ScrolledWindow,
    mode: ViewMode,
) {
    let albums = match state.storage.get_all_albums().await {
        Ok(a) => a,
        Err(e) => {
            info!(error = %e, "Failed to load albums");
            return;
        }
    };

    if albums.is_empty() {
        let empty_widget = build_empty_state(
            state,
            &EmptyStateParams {
                icon_name: "folder-music-symbolic",
                icon_label: "Music library icon",
                heading: "No Albums Found",
                heading_label: "No albums found",
                description: "Add a music folder to see your albums here.",
                description_label: "Add a music folder to see your albums here.",
            },
        );
        scrolled.set_child(Some(&empty_widget));
        return;
    }

    let mut artist_names: HashMap<i64, String> = HashMap::new();
    for album in &albums {
        if artist_names.contains_key(&album.artist_id) {
            continue;
        }
        let Ok(Some(artist)) = state.storage.get_artist(album.artist_id).await else {
            continue;
        };
        artist_names.insert(album.artist_id, artist.name);
    }

    let cards: Vec<Widget> = albums
        .iter()
        .map(|album| {
            let artist_name = artist_names
                .get(&album.artist_id)
                .map_or("Unknown Artist", String::as_str);
            build_album_card(state, album, artist_name).upcast()
        })
        .collect();

    match mode {
        Grid => populate_grid(
            container,
            "Album library grid \u{2014} click an album to play",
            cards,
        ),
        Column => populate_list(
            container,
            "Album library list \u{2014} click an album to play",
            cards,
        ),
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
/// Returns a `Button` containing a vertical layout with cover art,
/// title, artist, format summary, and year labels.
fn build_album_card(state: &Arc<AppState>, album: &Album, artist_name: &str) -> Button {
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
        .halign(Start)
        .build();

    let cover_art = build_cover_art(album);

    let overlay = Overlay::new();
    overlay.set_child(Some(&cover_art));
    overlay.set_css_classes(&["cover-overlay"]);

    let play_button = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .css_classes(["circular", "osd"])
        .halign(Center)
        .valign(Center)
        .build();
    play_button.set_visible(false);

    overlay.add_overlay(&play_button);

    let motion_ctrl = EventControllerMotion::new();
    let btn_show = play_button.clone();
    motion_ctrl.connect_enter(move |_, _, _| {
        btn_show.set_visible(true);
    });
    let btn_hide = play_button.clone();
    motion_ctrl.connect_leave(move |_| {
        btn_hide.set_visible(false);
    });
    overlay.add_controller(motion_ctrl);

    let state_clone = Arc::clone(state);
    let album_id = album.id;
    play_button.connect_clicked(move |_| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            play_album(&state, album_id).await;
        });
    });

    content.append(&overlay.upcast::<Widget>());

    let title_label = Label::builder()
        .label(&album.title)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();

    let artist_label = Label::builder()
        .label(artist_name)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();

    let format_row = Box::builder().orientation(Horizontal).spacing(6).build();

    let format_label = Label::builder()
        .label(&album.format_summary)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(14)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    format_label.set_hexpand(true);

    let year_label = Label::builder()
        .label(album.year.map_or(String::new(), |y| y.to_string()))
        .css_classes(["dim-label", "caption"])
        .halign(End)
        .build();

    format_row.append(&format_label);
    format_row.append(&year_label);

    content.append(&title_label);
    content.append(&artist_label);
    content.append(&format_row);

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

    let track_paths: HashMap<i64, PathBuf> = tracks
        .iter()
        .map(|t| (t.id, PathBuf::from(&t.audio.file_path)))
        .collect();
    let track_ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();

    state.playback.set_track_paths(track_paths);

    if let Err(e) = state.playback.play_queue(track_ids) {
        let error_str = e.to_string();
        info!(error = %error_str, album_id, "Failed to start album playback");
        let msg = match &e {
            PlaybackNoDeviceAvailable
            | PlaybackDeviceDisconnected
            | Output(NoDeviceAvailable | DeviceDisconnected(_)) => {
                "No audio device available. Check your audio output."
            }
            _ => &error_str,
        };
        if let Err(e) = state.toast_tx.send(msg.into()).await {
            info!(error = %e, "Failed to enqueue toast notification");
        }
    }
}

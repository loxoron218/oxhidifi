//! Album grid view with cover art thumbnails.
//!
//! Displays albums in a responsive `FlowBox` grid. Each album cell shows
//! cover art (or a placeholder icon), title, and artist name. Clicking an
//! album triggers playback via the `PlaybackController`.
//! Shows an inline empty state when no albums are available.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Sender, channel},
    },
};

use {
    libadwaita::{
        glib::{
            ControlFlow::{Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{
            Align::{Center, End, Start},
            Box, Button,
            ContentFit::Cover,
            EventControllerMotion, GestureClick, Image, Label,
            Orientation::{Horizontal, Vertical},
            Overlay, Picture, ScrolledWindow, Widget,
            gdk::{
                MemoryFormat::{R8g8b8, R8g8b8a8},
                MemoryTexture,
            },
            gdk_pixbuf::Pixbuf,
            gio::spawn_blocking,
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

    let artist_names: HashMap<i64, String> = match state.storage.get_all_artists().await {
        Ok(artists) => artists.into_iter().map(|a| (a.id, a.name)).collect(),
        Err(e) => {
            info!(error = %e, "Failed to load artists");
            HashMap::new()
        }
    };

    let mut cover_art_data: Vec<(usize, String)> = Vec::new();
    let mut overlays: Vec<Overlay> = Vec::new();

    let cards: Vec<Widget> = albums
        .iter()
        .enumerate()
        .map(|(i, album)| {
            let artist_name = artist_names
                .get(&album.artist_id)
                .map_or("Unknown Artist", String::as_str);
            let (card, overlay) = build_album_card(state, album, artist_name);
            if let Some(path) = &album.artwork_path {
                cover_art_data.push((i, path.clone()));
            }
            overlays.push(overlay);
            card.upcast()
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

    load_cover_art_async(&cover_art_data, &overlays);
}

/// Build a placeholder cover art widget.
///
/// Returns an `Image` with a generic audio icon. Used as the initial
/// state before async cover art loading completes.
fn build_placeholder() -> Widget {
    Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(THUMBNAIL_SIZE / 2)
        .width_request(THUMBNAIL_SIZE)
        .height_request(THUMBNAIL_SIZE)
        .css_classes(["album-cover", "dim-label"])
        .build()
        .upcast()
}

/// Decode an image file at thumbnail size on a background thread.
///
/// Returns a `MemoryTexture` suitable for painting, or `None` if the
/// file could not be loaded or decoded.
fn decode_cover_art(path: &str) -> Option<MemoryTexture> {
    let pixbuf = Pixbuf::from_file_at_scale(path, THUMBNAIL_SIZE, THUMBNAIL_SIZE, true).ok()?;
    let format = if pixbuf.has_alpha() { R8g8b8a8 } else { R8g8b8 };
    let bytes = pixbuf.read_pixel_bytes();
    Some(MemoryTexture::new(
        pixbuf.width(),
        pixbuf.height(),
        format,
        &bytes,
        pixbuf.rowstride().cast_unsigned() as usize,
    ))
}

/// Decode a batch of cover art images and send results through a channel.
fn decode_batch(batch: Vec<(usize, String)>, tx: &Sender<(usize, Option<MemoryTexture>)>) {
    for (i, path) in batch {
        let texture = decode_cover_art(&path);
        let _ = tx.send((i, texture));
    }
}

/// Apply a decoded texture to an overlay's child.
///
/// If the child is already a `Picture`, updates its paintable in place.
/// Otherwise replaces the child with a new `Picture`.
fn apply_texture(overlay: &Overlay, texture: &MemoryTexture) {
    let updated = overlay.child().and_then(|c| {
        c.downcast_ref::<Picture>()
            .map(|p| p.set_paintable(Some(texture)))
    });
    if updated.is_none() {
        let picture = Picture::builder()
            .paintable(texture)
            .content_fit(Cover)
            .width_request(THUMBNAIL_SIZE)
            .height_request(THUMBNAIL_SIZE)
            .css_classes(["album-cover"])
            .build();
        overlay.set_child(Some(&picture));
    }
}

/// Load cover art images off the main thread and apply them to overlays.
///
/// Splits work across multiple `gio::spawn_blocking` workers for parallel
/// decoding. Uses `std::sync::mpsc` to send results back and
/// `glib::idle_add_local` to apply textures on the `GLib` main thread.
fn load_cover_art_async(cover_art_data: &[(usize, String)], overlays: &[Overlay]) {
    if cover_art_data.is_empty() {
        return;
    }

    let (tx, rx) = channel();
    let total = cover_art_data.len();
    let num_workers = 4.min(total);

    let mut paths: Vec<(usize, String)> = cover_art_data.to_vec();
    let batch_size = total.div_ceil(num_workers);

    for _ in 0..num_workers {
        let batch: Vec<(usize, String)> = paths.drain(..batch_size.min(paths.len())).collect();
        let tx = tx.clone();
        spawn_blocking(move || decode_batch(batch, &tx));
    }
    drop(tx);

    let overlays: Vec<Overlay> = overlays.to_vec();
    let mut received = 0usize;

    idle_add_local(move || {
        while let Ok((i, Some(texture))) = rx.try_recv() {
            apply_texture(&overlays[i], &texture);
            received += 1;
        }
        if received < total { Continue } else { Break }
    });
}

/// Build a single album card widget.
///
/// Returns a `Box` containing a vertical layout with cover art,
/// title, artist, format summary, and year labels. Uses
/// `GestureClick` for click handling instead of `Button` to avoid
/// theme-inflated natural sizing from the `card` CSS class.
///
/// Also returns the `Overlay` wrapping the cover art so it can be
/// updated asynchronously after the card is added to the container.
fn build_album_card(state: &Arc<AppState>, album: &Album, artist_name: &str) -> (Box, Overlay) {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .tooltip_text(format!(
            "Play \u{201c}{}\u{201d} by album artist",
            album.title
        ))
        .build();

    let cover_art = build_placeholder();

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

    card.append(&overlay.clone().upcast::<Widget>());

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

    card.append(&title_label);
    card.append(&artist_label);
    card.append(&format_row);

    let gesture = GestureClick::new();
    let state_clone = Arc::clone(state);
    let album_id = album.id;
    gesture.connect_released(move |_, _, _, _| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            play_album(&state, album_id).await;
        });
    });
    card.add_controller(gesture);

    (card, overlay)
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

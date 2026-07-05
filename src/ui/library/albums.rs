//! Album grid/column view.
//!
//! Displays albums in a responsive `FlowBox` grid (grid mode) or a
//! sortable `GtkColumnView` (column mode). Only the *initial* mode is
//! built at startup; the other mode is lazily built on first switch.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::Ordering::Relaxed,
        mpsc::{Sender, channel},
    },
};

use {
    libadwaita::{
        gdk::MemoryTexture,
        gio::spawn_blocking,
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
            Overlay, Picture, Stack, Widget,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{BoxExt, ButtonExt, WidgetExt},
    },
    tokio::join,
    tracing::{error, info},
};

use crate::{
    app::{AppState, NavigationEvent::AlbumDetail},
    playback::{
        OutputError::{DeviceDisconnected, NoDeviceAvailable},
        PlaybackError::{
            DeviceDisconnected as PlaybackDeviceDisconnected,
            NoDeviceAvailable as PlaybackNoDeviceAvailable, Output,
        },
        engine::{PlaybackController, PlaybackStatus::Playing},
    },
    storage::{
        Album, Storage,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::{
        decode_cover_at_size,
        library::{
            column_view::{NarrowState, build_album_column_view},
            common::populate_grid_batched,
            empty::{
                EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
            },
        },
    },
};

/// Size of album cover art thumbnails in pixels.
const THUMBNAIL_SIZE: i32 = 180;

/// Build the album grid view.
///
/// Creates a `LibraryGrid` that holds both grid (`FlowBox`) and column
/// (`ColumnView`) layouts in a `Stack`.  Data is fetched once; switching
/// between modes is a fast `set_visible_child_name` call.
///
/// # Arguments
///
/// * `state` - Application state
/// * `narrow_mode` - Narrow‑mode tracker for adaptive column hiding
#[must_use]
pub fn build_album_grid(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> LibraryGrid {
    let nm = Arc::clone(narrow_state);
    build_library_grid(
        state,
        "Album library grid \u{2014} click an album to play",
        &nm,
        |state, narrow_state, initial_mode| {
            let stack = Stack::new();
            let stack_clone = stack.clone();
            spawn_future_local(async move {
                populate_album_views(&state, &stack_clone, &narrow_state, initial_mode).await;
            });
            stack
        },
    )
}

/// Fetch album data and build **only the initial** view mode into `stack`.
///
/// Delegates to [`lazy_build_album_mode`] which handles the fetch–
/// empty–build–set cycle.
async fn populate_album_views(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    initial_mode: ViewMode,
) {
    lazy_build_album_mode(state, stack, narrow_state, initial_mode).await;
}

/// Build the given `mode` view (grid or column) and add it to `stack`.
///
/// Each mode is wrapped in its own `ScrolledWindow` so scroll positions
/// are kept independent.  The other mode is NOT built here — it will be
/// lazily built on first toggle via [`lazy_build_album_mode`].
fn build_album_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &NarrowState,
    mode: ViewMode,
    albums: &[Album],
    artist_names: &HashMap<i64, String>,
) {
    match mode {
        Grid => {
            let mut overlays: Vec<Overlay> = Vec::new();

            let cover_art_data: Vec<(usize, String)> = albums
                .iter()
                .enumerate()
                .filter_map(|(i, album)| album.artwork_path.as_ref().map(|path| (i, path.clone())))
                .collect();

            let cards: Vec<Widget> = albums
                .iter()
                .map(|album| {
                    let artist_name = artist_names
                        .get(&album.artist_id)
                        .map_or("Unknown Artist", String::as_str);
                    let (card, overlay) = build_album_card(state, album, artist_name);
                    overlays.push(overlay);
                    card.upcast()
                })
                .collect();

            let grid_container = Box::builder().orientation(Vertical).build();
            let mut remaining = cards;
            populate_grid_batched(
                &grid_container,
                &mut remaining,
                50,
                "Album library grid \u{2014} click an album to play",
            );

            load_cover_art_async(&cover_art_data, &overlays);

            add_scrolled(stack, &grid_container, "grid");
        }
        Column => {
            let column_view = build_album_column_view(state, albums, artist_names, narrow_state);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Lazily build a view mode that wasn't constructed at startup.
///
/// Re‑fetches data from storage, builds the requested `mode` widget,
/// adds it to `stack`, and switches to it.  This is a no‑op if the
/// child already exists (race‑guard).
pub async fn lazy_build_album_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    mode: ViewMode,
) {
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    let (albums_res, artist_names_res) = join!(
        state.storage.get_all_albums(),
        state.storage.get_all_artists(),
    );

    let albums = match albums_res {
        Ok(a) => a,
        Err(e) => {
            info!(error = %e, "Failed to load albums for lazy build");
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
        stack.add_named(&empty_widget, Some("grid"));
        stack.set_visible_child_name("grid");
        return;
    }

    let artist_names: HashMap<i64, String> = match artist_names_res {
        Ok(artists) => artists.into_iter().map(|a| (a.id, a.name)).collect(),
        Err(e) => {
            info!(error = %e, "Failed to load artists for lazy build");
            HashMap::new()
        }
    };

    build_album_mode(state, stack, narrow_state, mode, &albums, &artist_names);
    stack.set_visible_child_name(child_name);
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
    decode_cover_at_size(path, THUMBNAIL_SIZE)
}

/// Decode a batch of cover art images and send results through a channel.
fn decode_batch(batch: Vec<(usize, String)>, tx: &Sender<(usize, Option<MemoryTexture>)>) {
    for (i, path) in batch {
        let texture = decode_cover_art(&path);
        if let Err(e) = tx.send((i, texture)) {
            error!(error = %e, "Failed to send cover art texture");
        }
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

    let album_id = album.id;

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
        let btn = btn_click.clone();
        spawn_future_local(async move {
            toggle_or_play_album(&state, album_id).await;
            btn.set_icon_name(album_play_icon(&state, album_id));
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
    gesture.connect_released(move |_, _, _, _| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            state.send_navigation_event(AlbumDetail(album_id)).await;
        });
    });
    card.add_controller(gesture);

    (card, overlay)
}

/// Determine the overlay button icon for an album based on playback state.
fn album_play_icon(state: &AppState, album_id: i64) -> &'static str {
    let is_current = state.current_album_id.load(Relaxed) == album_id;
    let ps = state.playback.state();
    if is_current && ps.status == Playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }
}

/// Toggle pause if this album is currently playing, otherwise play it.
async fn toggle_or_play_album(state: &Arc<AppState>, album_id: i64) {
    let is_current = state.current_album_id.load(Relaxed) == album_id;
    if is_current {
        if let Err(e) = state.playback.toggle_pause() {
            error!(error = %e, "Failed to toggle pause");
        }
    } else {
        play_album(state, album_id).await;
    }
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

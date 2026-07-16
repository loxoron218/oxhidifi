//! Album grid/column view.
//!
//! Displays albums in a responsive `FlowBox` grid (grid mode) or a
//! sortable `GtkColumnView` (column mode). Only the *initial* mode is
//! built at startup; the other mode is lazily built on first switch.

use std::{boxed::Box, collections::HashMap, path::PathBuf, sync::Arc};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gdk::{MemoryTexture, prelude::TextureExt},
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{
            Align::{Center, End, Start},
            Box as GtkBox, Button,
            ContentFit::Cover,
            EventControllerMotion, FlowBox, GestureClick, Image, Label,
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
        Album, FormatInfo, Storage,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::{
        ArtworkDecodeRequest, CoverArtCache, DecodedCover,
        library::{
            column_view::{NarrowState, build_album_column_view},
            common::build_grid,
            empty::{
                EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
            },
        },
        raw_to_texture,
    },
};

/// Number of album cards to build per idle callback batch.
const GRID_BATCH_SIZE: usize = 10;

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
        |stack: &Stack, state, narrow_state, initial_mode| {
            let stack_clone = stack.clone();
            spawn_future_local(async move {
                populate_album_views(&state, &stack_clone, &narrow_state, initial_mode).await;
            });
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

/// Populate up to `GRID_BATCH_SIZE` album cards into the flow box.
/// Returns `Continue` if more items remain, `Break` when done.
fn fill_album_grid(
    snapshots: &mut Vec<(Album, String, FormatInfo)>,
    overlays: &mut Vec<Overlay>,
    cover_art_data: &mut Vec<(i64, usize, String)>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    cache: &Arc<CoverArtCache>,
) {
    for _ in 0..GRID_BATCH_SIZE {
        let Some((album, artist_name, fi)) = snapshots.pop() else {
            break;
        };
        let index = overlays.len();
        let (card, overlay) = build_album_card(state, &album, &artist_name, &fi);
        if let Some(path) = &album.artwork_path {
            cover_art_data.push((album.id, index, path.clone()));
        }
        overlays.push(overlay);
        flow.append(&card.upcast::<Widget>());
    }
    if snapshots.is_empty() {
        load_cover_art_async(cover_art_data, overlays, cache);
    }
}

/// Check if the snapshots are exhausted and return the appropriate `ControlFlow`.
fn check_done(snapshots: &[(Album, String, FormatInfo)]) -> ControlFlow {
    if snapshots.is_empty() {
        Break
    } else {
        Continue
    }
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
    format_info: &HashMap<i64, FormatInfo>,
) {
    match mode {
        Grid => {
            let grid_container = GtkBox::builder().orientation(Vertical).build();
            let flow = build_grid("Album library grid \u{2014} click an album to play");
            grid_container.append(&flow);
            add_scrolled(stack, &grid_container, "grid");

            let state = Arc::clone(state);
            let cache = Arc::clone(&state.cover_art_cache);

            let mut snapshots: Vec<(Album, String, FormatInfo)> = albums
                .iter()
                .rev()
                .map(|album| {
                    let artist_name = artist_names
                        .get(&album.artist_id)
                        .map_or_else(|| "Unknown Artist".to_string(), Clone::clone);
                    let fi = format_info.get(&album.id).cloned().unwrap_or_default();
                    (album.clone(), artist_name, fi)
                })
                .collect();

            let mut overlays: Vec<Overlay> = Vec::new();
            let mut cover_art_data: Vec<(i64, usize, String)> = Vec::new();

            idle_add_local(move || {
                fill_album_grid(
                    &mut snapshots,
                    &mut overlays,
                    &mut cover_art_data,
                    &flow,
                    &state,
                    &cache,
                );
                check_done(&snapshots)
            });
        }
        Column => {
            let column_view =
                build_album_column_view(state, albums, artist_names, narrow_state, format_info);
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

    let album_ids: Vec<i64> = albums.iter().map(|a| a.id).collect();
    let format_info = state
        .storage
        .get_albums_format_info(&album_ids)
        .await
        .unwrap_or_default();

    let artist_names: HashMap<i64, String> = match artist_names_res {
        Ok(artists) => artists.into_iter().map(|a| (a.id, a.name)).collect(),
        Err(e) => {
            info!(error = %e, "Failed to load artists for lazy build");
            HashMap::new()
        }
    };

    build_album_mode(
        state,
        stack,
        narrow_state,
        mode,
        &albums,
        &artist_names,
        &format_info,
    );
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

/// Checks the shared [`CoverArtCache`] first; sends decode requests to the
/// centralized worker when the texture is not yet cached.  Each decoded
/// cover is written to the cache and applied to its overlay via a
/// [`spawn_future_local`] async task.
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

/// Checks the shared [`CoverArtCache`] first; sends decode requests to the
/// centralized worker when the texture is not yet cached.  Each decoded
/// cover is written to the cache and applied to its overlay via a
/// [`spawn_future_local`] async task that stays alive until all results
/// are received, preventing a race where the channel receiver is dropped
/// before the background decoder finishes.
fn load_cover_art_async(
    cover_art_data: &[(i64, usize, String)],
    overlays: &[Overlay],
    cache: &Arc<CoverArtCache>,
) {
    if cover_art_data.is_empty() {
        return;
    }

    let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
    let mut uncached: Vec<(i64, usize, String)> = Vec::new();

    for (album_id, index, path) in cover_art_data {
        if let Some(texture) = cache
            .get(*album_id)
            .filter(|t| t.width() >= THUMBNAIL_SIZE || t.height() >= THUMBNAIL_SIZE)
        {
            apply_texture(&overlays[*index], &texture);
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
            size: THUMBNAIL_SIZE,
            on_complete: Box::new(move |_aid, decoded| {
                try_send_album_cover(&tx, index, album_id, decoded);
            }),
        });
    }
    drop(tx);

    let overlays: Vec<Overlay> = overlays.to_vec();
    let cache_clone = Arc::clone(cache);

    spawn_future_local(async move {
        while let Ok((index, album_id, decoded)) = rx.recv().await {
            let texture = raw_to_texture(&decoded);
            cache_clone.insert(album_id, texture.clone());
            apply_texture(&overlays[index], &texture);
        }
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
fn build_album_card(
    state: &Arc<AppState>,
    album: &Album,
    artist_name: &str,
    format_info: &FormatInfo,
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
        spawn_future_local(async move {
            toggle_or_play_album(&state, album_id).await;
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

    let format_row = GtkBox::builder().orientation(Horizontal).spacing(6).build();

    let format_label = Label::builder()
        .label(format_info.summary())
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
    let ps = state.playback.state();
    let is_current = ps.current_album_id == album_id;
    if is_current && ps.status == Playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }
}

/// Toggle pause if this album is currently playing, otherwise play it.
async fn toggle_or_play_album(state: &Arc<AppState>, album_id: i64) {
    let is_current = state.playback.state().current_album_id == album_id;
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

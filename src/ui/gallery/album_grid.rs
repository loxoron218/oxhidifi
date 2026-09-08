//! Album grid/column view.
//!
//! Displays albums in a responsive `FlowBox` grid (grid mode) or a
//! sortable `GtkColumnView` (column mode). Only the *initial* mode is
//! built at startup; the other mode is lazily built on first switch.

use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering::Relaxed},
};

use {
    libadwaita::{
        glib::{idle_add_local, spawn_future_local},
        gtk::{Box, Orientation::Vertical, Overlay, Stack},
        prelude::BoxExt,
    },
    tokio::join,
    tracing::warn,
};

use crate::{
    app::runtime::{AppState, CachedAlbumData, NavigationEvent::AlbumDetail},
    storage::{
        Storage,
        active_tab::ActiveTab::Albums,
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::{
        gallery::{
            coalescer::spawn_listen_sort_zoom,
            empty::{LibraryGrid, add_scrolled, build_library_grid, show_library_empty},
            frame_resize::{apply_album_resize, fill_album_grid, resize_album_grid},
            grid_flow::build_grid,
            keyboard_nav::setup_flowbox_keyboard_nav,
            narrow_flag::NarrowState,
            priority::album_sort_indices,
            rebuild_debounce::{
                RebuildAction::{DeferDirty, Resize},
                decide_rebuild,
            },
            stack_cache::{clear_mode_children, take_stale_mode_child, try_build_from_cache},
            table::build_album_column_view,
        },
        zoom::grid_cover_size,
    },
};

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
pub fn build_album_grid(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> LibraryGrid {
    let nm = Arc::clone(narrow_state);
    let lg = build_library_grid(
        state,
        &nm,
        |stack: &Stack, state, narrow_state, initial_mode| {
            lazy_build_album_mode(&state, stack, &narrow_state, initial_mode);
        },
    );

    let grid_state = Arc::clone(state);
    let grid_stack = lg.mode_stack.clone();
    let grid_narrow = Arc::clone(narrow_state);
    let preview_state = Arc::clone(state);
    let preview_stack = lg.mode_stack.clone();
    spawn_listen_sort_zoom(
        state,
        state.albums_sort_rx.clone(),
        state.albums_zoom_rx.clone(),
        move || {
            if preview_state.active_tab.borrow() == Albums
                && preview_state.view_mode.borrow() == Grid
                && preview_state.album_grid.ready.load(Relaxed)
            {
                _ = apply_album_resize(&preview_state, &preview_stack);
            }
        },
        move |sort_fired, zoom_fired| {
            rebuild_album_current_mode(
                &grid_state,
                &grid_stack,
                &grid_narrow,
                sort_fired,
                zoom_fired,
            );
        },
    );

    lg
}

/// Rebuild or resize the album grid after a sort/zoom change.
///
/// Zoom-only changes resize the existing cards in place (no widget churn,
/// scroll position preserved). Sort changes — or any state that invalidates
/// the current cards — fall back to a full rebuild from the in-memory cache.
/// When the tab is hidden, marks the grid dirty so it is rebuilt on switch.
fn rebuild_album_current_mode(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    sort_fired: bool,
    zoom_fired: bool,
) {
    let action = decide_rebuild(
        state.active_tab.borrow(),
        Albums,
        state.view_mode.borrow(),
        sort_fired,
        zoom_fired,
        state.album_grid.ready.load(Relaxed),
    );
    if action == DeferDirty {
        state.album_grid.dirty.store(true, Relaxed);
        return;
    }
    if action == Resize && resize_album_grid(state, mode_stack) {
        return;
    }
    let Some(mode) = take_stale_mode_child(state, Albums, mode_stack) else {
        state.album_grid.dirty.store(true, Relaxed);
        return;
    };
    if sort_fired {
        clear_mode_children(mode_stack);
    }
    state.album_grid.dirty.store(false, Relaxed);
    lazy_build_album_mode(state, mode_stack, narrow_state, mode);
}

/// Build the given `mode` view (grid or column) and add it to `stack`.
///
/// Borrows the shared album data from `cached` and derives the display order
/// from `indices`, so no deep clone is performed. Each mode is wrapped in
/// its own `ScrolledWindow` so scroll positions are kept independent.
/// The other mode is NOT built here — it will be lazily built on first
/// toggle via [`lazy_build_album_mode`].
///
/// # Arguments
///
/// * `cached` - Shared album data (albums, artist names, format info)
/// * `indices` - Display order of `cached.albums` (e.g. a sorted index vector)
fn build_album_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &NarrowState,
    mode: ViewMode,
    cached: &CachedAlbumData,
    indices: &[usize],
) {
    let cover_size = grid_cover_size(state.storage.get_grid_zoom_level());
    match mode {
        Grid => {
            state.album_grid.ready.store(false, Relaxed);
            *state.album_grid_covers.lock() = Arc::new(Vec::new());
            let grid_container = Box::builder().orientation(Vertical).build();
            let flow = build_grid(
                "Album library grid \u{2014} click an album to play",
                cover_size,
            );
            grid_container.append(&flow);
            add_scrolled(stack, &grid_container, "grid");

            let album_ids: Vec<i64> = indices
                .iter()
                .filter_map(|&i| cached.albums.get(i).map(|album| album.id))
                .collect();
            setup_flowbox_keyboard_nav(&flow, state, album_ids, AlbumDetail);

            let cached_owned = CachedAlbumData {
                albums: Arc::clone(&cached.albums),
                artist_names: Arc::clone(&cached.artist_names),
                format_info: Arc::clone(&cached.format_info),
            };
            let state_fill = Arc::clone(state);
            let build_seq = state_fill
                .album_grid
                .build_seq
                .fetch_add(1, Relaxed)
                .wrapping_add(1);
            let mut overlays: Vec<Overlay> = Vec::new();
            let mut cover_art_data: Vec<(i64, usize, String)> = Vec::new();
            let mut remaining: Vec<usize> = indices.iter().rev().copied().collect();

            state.handles.lock().retain_source(idle_add_local(move || {
                fill_album_grid(
                    &cached_owned,
                    &mut remaining,
                    &mut overlays,
                    &mut cover_art_data,
                    &flow,
                    &state_fill,
                    build_seq,
                )
            }));
        }
        Column => {
            let column_view = build_album_column_view(state, cached, narrow_state, indices);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Lazily build a view mode that wasn't constructed at startup.
///
/// Re‑fetches data from storage, builds the requested `mode` widget,
/// adds it to `stack`, and switches to it.  This is a no‑op if the
/// child already exists (race‑guard).  The synchronous front half runs
/// the race‑guard and cached‑build checks; the storage re‑fetch and
/// widget construction run in a local future on the `GLib` main context.
pub fn lazy_build_album_mode(
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

    if try_build_from_cache(
        &state.album_grid.cache,
        stack,
        child_name,
        |cached| cached.albums.is_empty(),
        |cached| {
            let indices = album_sort_indices(state, cached);
            build_album_mode(state, stack, narrow_state, mode, cached, &indices);
        },
        || show_albums_empty(state, stack),
    ) {
        return;
    }

    let state_cb = Arc::clone(state);
    let stack_cb = stack.clone();
    let narrow_cb = Arc::clone(narrow_state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let my_gen = state_cb.album_grid.generation.load(Relaxed);
            let (albums_res, artist_names_res) = join!(
                state_cb.storage.get_all_albums(),
                state_cb.storage.get_all_artists(),
            );

            let albums = match albums_res {
                Ok(a) => a,
                Err(e) => {
                    warn!(error = %e, "Failed to load albums for lazy build");
                    return;
                }
            };

            let artist_names: HashMap<i64, String> = match artist_names_res {
                Ok(artists) => artists.into_iter().map(|a| (a.id, a.name)).collect(),
                Err(e) => {
                    warn!(error = %e, "Failed to load artists for lazy build");
                    HashMap::new()
                }
            };

            let format_info = if albums.is_empty() {
                HashMap::new()
            } else {
                let album_ids: Vec<i64> = albums.iter().map(|a| a.id).collect();
                state_cb
                    .storage
                    .get_albums_format_info(&album_ids)
                    .await
                    .unwrap_or_default()
            };

            let cached = CachedAlbumData {
                albums: Arc::new(albums),
                artist_names: Arc::new(artist_names),
                format_info: Arc::new(format_info),
            };
            finish_lazy_album_build(&state_cb, &stack_cb, &narrow_cb, mode, my_gen, cached);
        }));
}

/// Complete a lazy album-mode build from already-fetched data.
///
/// Applies the generation and child-existence race guards, then either shows
/// the empty state or builds the mode widget from the fetched data.
fn finish_lazy_album_build(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    mode: ViewMode,
    my_gen: u64,
    data: CachedAlbumData,
) {
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    if data.albums.is_empty() {
        if state.album_grid.generation.load(Relaxed) != my_gen {
            return;
        }
        if stack.child_by_name("grid").is_some() {
            stack.set_visible_child_name("grid");
            return;
        }
        *state.album_grid.cache.lock() = Some(CachedAlbumData {
            albums: data.albums,
            artist_names: Arc::new(HashMap::new()),
            format_info: Arc::new(HashMap::new()),
        });
        show_albums_empty(state, stack);
        return;
    }

    if state.album_grid.generation.load(Relaxed) != my_gen {
        return;
    }
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    let indices = album_sort_indices(state, &data);
    build_album_mode(state, stack, narrow_state, mode, &data, &indices);
    *state.album_grid.cache.lock() = Some(data);
    stack.set_visible_child_name(child_name);
}

/// Show the empty‑albums state widget, replacing any existing grid child.
///
/// Two-state per FR-007: "No Music Library Configured" when no directories
/// are configured, "No Music Found" when directories exist but contain no
/// albums.
fn show_albums_empty(state: &Arc<AppState>, stack: &Stack) {
    state.album_grid.ready.store(false, Relaxed);
    *state.album_grid_covers.lock() = Arc::new(Vec::new());
    show_library_empty(state, stack, "folder-music-symbolic", "Music library icon");
}

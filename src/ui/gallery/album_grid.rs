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
        glib::idle_add_local,
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
            coalescer::spawn_grid_sort_zoom,
            cover_dispatch::album_covers_ready,
            empty::{LibraryGrid, add_scrolled, build_library_grid, show_library_empty},
            frame_resize::{
                AlbumFillContext, apply_album_resize, fill_album_grid, resize_album_grid,
            },
            grid_fit::{grid_is_current, upgrade_hidden_resize},
            grid_flow::build_grid,
            keyboard_nav::setup_flowbox_keyboard_nav,
            narrow_flag::NarrowState,
            priority::album_sort_indices,
            rebuild_debounce::{
                RebuildAction::{DeferDirty, Resize},
                decide_rebuild,
            },
            stack_cache::{
                GridBuildTarget, clear_mode_children, mode_child_name, spawn_grid_fetch,
                take_stale_mode_child,
            },
            table::build_album_column_view,
        },
        zoom::effective_grid_cover_size,
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

    spawn_grid_sort_zoom(
        state,
        &lg.mode_stack,
        narrow_state,
        state.albums_sort_rx.clone(),
        state.albums_zoom_rx.clone(),
        preview_album_resize,
        rebuild_album_current_mode,
    );

    lg
}

/// Resolve the album cover size to render for the current narrow state.
///
/// Snaps the stored zoom level down on narrow windows so two columns keep
/// fitting the minimum window width; the stored preference is untouched.
///
/// # Arguments
///
/// * `state` - Application state (stored zoom level source).
/// * `narrow_state` - Narrow-width tracker.
///
/// # Returns
///
/// Cover art size in pixels to render.
fn effective_album_cover(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> i32 {
    effective_grid_cover_size(state.storage.get_grid_zoom_level(), narrow_state.get())
}

/// Preview a zoom or narrow-window change on the live album grid.
///
/// Resizes cards immediately (without cover decoding) when the grid is
/// visible and ready; the debounced rebuild dispatches decoding afterwards.
/// Skips grids already rendering at the effective size.
fn preview_album_resize(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    if state.active_tab.borrow() != Albums
        || state.view_mode.borrow() != Grid
        || !state.album_grid.ready.load(Relaxed)
    {
        return;
    }
    let size = effective_album_cover(state, narrow_state);
    if !grid_is_current(mode_stack, size) {
        _ = apply_album_resize(state, mode_stack, size);
    }
}

/// Rebuild or resize the album grid after a sort/zoom change.
///
/// Zoom-only changes resize the existing cards in place (no widget churn,
/// scroll position preserved) — including on hidden tabs, so a stale
/// oversized page never forces the window past its minimum. The in-place
/// resize is skipped only when both the geometry and the exact-size covers
/// are already current: the preview flips geometry without decoding, so a
/// geometry-only check would strand cards on oversized interim textures.
/// Sort changes — or any state that invalidates the current cards — fall back
/// to a full rebuild from the in-memory cache. When the tab is hidden, marks
/// the grid dirty so it is rebuilt on switch. Narrow-window changes arrive
/// here as zoom notifications (see `wire_narrow_fit`) and take the same
/// in-place path.
fn rebuild_album_current_mode(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    sort_fired: bool,
    zoom_fired: bool,
) {
    let action = upgrade_hidden_resize(
        decide_rebuild(
            state.active_tab.borrow(),
            Albums,
            state.view_mode.borrow(),
            sort_fired,
            zoom_fired,
            state.album_grid.ready.load(Relaxed),
        ),
        sort_fired,
        zoom_fired,
        state.view_mode.borrow(),
        state.album_grid.ready.load(Relaxed),
        mode_stack,
    );
    if action == DeferDirty {
        state.album_grid.dirty.store(true, Relaxed);
        return;
    }
    if action == Resize {
        let size = effective_album_cover(state, narrow_state);
        if grid_is_current(mode_stack, size) && album_covers_ready(state, size) {
            return;
        }
        if resize_album_grid(state, mode_stack, size) {
            return;
        }
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
    let cover_size =
        effective_grid_cover_size(state.storage.get_grid_zoom_level(), narrow_state.get());
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
                let ctx = AlbumFillContext {
                    flow: &flow,
                    state: &state_fill,
                    build_seq,
                    cover_size,
                };
                fill_album_grid(
                    &cached_owned,
                    &mut remaining,
                    &mut overlays,
                    &mut cover_art_data,
                    &ctx,
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
    spawn_grid_fetch(
        GridBuildTarget {
            state,
            stack,
            narrow_state,
            mode,
        },
        &state.album_grid.cache,
        |cached: &CachedAlbumData| cached.albums.is_empty(),
        |cached| {
            let indices = album_sort_indices(state, cached);
            build_album_mode(state, stack, narrow_state, mode, cached, &indices);
        },
        || show_albums_empty(state, stack),
        async move |fetch_state: Arc<AppState>,
                    fetch_stack: Stack,
                    fetch_narrow: Arc<NarrowState>,
                    fetch_mode: ViewMode| {
            let my_gen = fetch_state.album_grid.generation.load(Relaxed);
            let (albums_res, artist_names_res) = join!(
                fetch_state.storage.get_all_albums(),
                fetch_state.storage.get_all_artists(),
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
                fetch_state
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
            finish_lazy_album_build(
                &fetch_state,
                &fetch_stack,
                &fetch_narrow,
                fetch_mode,
                my_gen,
                cached,
            );
        },
    );
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
    let child_name = mode_child_name(mode);
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

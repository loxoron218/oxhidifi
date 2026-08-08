//! Album grid/column view.
//!
//! Displays albums in a responsive `FlowBox` grid (grid mode) or a
//! sortable `GtkColumnView` (column mode). Only the *initial* mode is
//! built at startup; the other mode is lazily built on first switch.

use std::{
    cmp::Ordering::{self, Equal},
    collections::HashMap,
    mem::take,
    sync::{Arc, atomic::Ordering::Relaxed},
};

use {
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{Box, FlowBox, Orientation::Vertical, Overlay, Stack, Widget},
        prelude::BoxExt,
    },
    tokio::join,
    tracing::warn,
};

use crate::{
    app::{AppState, CachedAlbumData, NavigationEvent::AlbumDetail},
    storage::{
        Storage,
        records::Album,
        settings::{
            ActiveTab::Albums,
            ViewMode::{self, Column, Grid},
        },
        sort_rules::{
            AlbumSortCriteria::{Artist, BitDepth, Format, SampleRate, Title, Year},
            AlbumSortItem,
            SortOrder::Descending,
        },
    },
    ui::{
        CoverArtCache,
        library::{
            album_card::{build_album_card, load_cover_art_async, resolve_cover_widget},
            column_view::build_album_column_view,
            common::{
                RebuildAction::{DeferDirty, Resize},
                build_grid, clear_mode_children, decide_rebuild, fill_grid_batch,
                memoized_sort_indices, resize_grid_batched, setup_flowbox_keyboard_nav,
                spawn_listen_sort_zoom, take_stale_mode_child, try_build_from_cache,
            },
            empty::{
                EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
            },
            narrow_state::NarrowState,
        },
    },
    zoom::grid_cover_size,
};

/// Snapshot of the album grid's cover state captured at resize time.
struct AlbumResizeSnapshot {
    /// Build sequence captured when the snapshot was taken.
    build_seq: u64,
    /// Reverse map: overlay index → album ID.
    idx_to_album: Arc<HashMap<usize, i64>>,
    /// Shared decoded-cover cache.
    cover_cache: Arc<CoverArtCache>,
    /// App state clone for the stale-build predicate.
    state: Arc<AppState>,
}

/// Compare two albums by a single sort item, applying the configured order.
fn cmp_albums(
    a: &Album,
    b: &Album,
    item: &AlbumSortItem,
    artist_names: &HashMap<i64, String>,
) -> Ordering {
    let mut cmp = match item.criteria {
        Title => a.title.cmp(&b.title),
        Artist => {
            let a_name = artist_names.get(&a.artist_id).map_or("", |s| s.as_str());
            let b_name = artist_names.get(&b.artist_id).map_or("", |s| s.as_str());
            a_name.cmp(b_name)
        }
        Year => a.year.cmp(&b.year),
        Format => a.format.cmp(&b.format),
        BitDepth => a.bit_depth.cmp(&b.bit_depth),
        SampleRate => a.sample_rate.cmp(&b.sample_rate),
    };
    if item.order == Descending {
        cmp = cmp.reverse();
    }
    cmp
}

/// Compute a display order for `albums` as a sorted index vector.
///
/// Single‑pass `sort_unstable_by` over indices — each criteria is checked
/// in priority order until a non‑equal comparison is found. Borrows the
/// album data so rebuilds never deep‑clone the `Vec<Album>`.
fn sorted_album_indices(
    albums: &[Album],
    artist_names: &HashMap<i64, String>,
    sort_items: &[AlbumSortItem],
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..albums.len()).collect();
    indices.sort_unstable_by(|&a, &b| {
        sort_items
            .iter()
            .find_map(|item| {
                let cmp = cmp_albums(&albums[a], &albums[b], item, artist_names);
                (cmp != Equal).then_some(cmp)
            })
            .unwrap_or(Equal)
    });
    indices
}

/// Memoized display order for the cached album data.
///
/// Returns the sort indices from the grid's memo when the library generation
/// and the current sort configuration both match, so repeated tab/mode
/// switches reuse the sort instead of re-comparing every album. Computes,
/// caches, and returns them otherwise.
fn album_sort_indices(state: &Arc<AppState>, cached: &CachedAlbumData) -> Arc<[usize]> {
    let generation = state.album_grid.generation.load(Relaxed);
    let config = state.storage.get_albums_sort();
    memoized_sort_indices(generation, config, &state.album_grid.memo, |config| {
        sorted_album_indices(&cached.albums, &cached.artist_names, config)
    })
}

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
            let stack_clone = stack.clone();
            spawn_future_local(async move {
                lazy_build_album_mode(&state, &stack_clone, &narrow_state, initial_mode).await;
            });
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
                apply_album_resize(&preview_state, &preview_stack);
            }
        },
        async move |sort_fired, zoom_fired| {
            rebuild_album_current_mode(
                &grid_state,
                &grid_stack,
                &grid_narrow,
                sort_fired,
                zoom_fired,
            )
            .await;
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
async fn rebuild_album_current_mode(
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
    lazy_build_album_mode(state, mode_stack, narrow_state, mode).await;
}

/// Snapshot the album grid's cover state for an in-place zoom resize.
///
/// Reads the build sequence, the cover data (`album ID`, `overlay index`,
/// `artwork path`) to derive the reverse map (overlay index → album ID), the
/// shared cover cache, and the app state into a single snapshot. Shared by
/// the preview and full resize paths so both resize the same live grid
/// without re-reading shared state.
fn album_resize_snapshot(state: &Arc<AppState>) -> AlbumResizeSnapshot {
    let build_seq = state.album_grid.build_seq.load(Relaxed);
    let cover_data = Arc::clone(&*state.album_grid_covers.lock());
    let idx_to_album: Arc<HashMap<usize, i64>> =
        Arc::new(cover_data.iter().map(|&(aid, idx, _)| (idx, aid)).collect());
    AlbumResizeSnapshot {
        build_seq,
        idx_to_album,
        cover_cache: Arc::clone(&state.cover_art_cache),
        state: Arc::clone(state),
    }
}

/// Build the per-card cover-resolve closure for a batched zoom resize.
///
/// Clones the reverse map and cover cache into a `'static` closure that
/// resolves each card's cover to the new size or a cached texture — without
/// dispatching new decode requests.
fn album_cover_resolver(
    idx_to_album: &Arc<HashMap<usize, i64>>,
    cover_cache: &Arc<CoverArtCache>,
) -> impl FnMut(usize, &Overlay, i32) + 'static {
    let idx_to_album = Arc::clone(idx_to_album);
    let cover_cache = Arc::clone(cover_cache);
    move |idx: usize, overlay: &Overlay, size: i32| {
        resolve_cover_at(&idx_to_album, &cover_cache, idx, overlay, size);
    }
}

/// Resize the live album grid's cards and resolve their cover widgets.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) that
/// resizes every card's cover to the current zoom level and resolves each
/// to its cached texture or a placeholder — without dispatching new decode
/// requests. Shared by the zoom preview (instant feedback without per-click
/// decode waves) and [`resize_album_grid`] (which adds the debounced decode
/// dispatch after the traversal completes).
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
fn apply_album_resize(state: &Arc<AppState>, mode_stack: &Stack) -> bool {
    let snapshot = album_resize_snapshot(state);
    let resolve_cards = album_cover_resolver(&snapshot.idx_to_album, &snapshot.cover_cache);
    let stale_state = Arc::clone(&snapshot.state);
    resize_grid_batched(
        state,
        mode_stack,
        move || stale_state.album_grid.build_seq.load(Relaxed) != snapshot.build_seq,
        resolve_cards,
        |_, _| {},
    )
}

/// Resize the live album grid's cards and dispatch cover decoding.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) and,
/// once the traversal completes, dispatches cover decoding for the new size
/// for any art not yet cached. The cover metadata (album ID → artwork path,
/// in card order) is read from state.
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
fn resize_album_grid(state: &Arc<AppState>, mode_stack: &Stack) -> bool {
    let snapshot = album_resize_snapshot(state);
    let resolve_cards = album_cover_resolver(&snapshot.idx_to_album, &snapshot.cover_cache);
    let dispatch_covers = {
        let state = Arc::clone(state);
        let cover_data = Arc::clone(&*state.album_grid_covers.lock());
        let cover_cache = Arc::clone(&snapshot.cover_cache);
        move |overlays: Vec<Overlay>, size| {
            load_cover_art_async(&state, &cover_data, &overlays, &cover_cache, size);
        }
    };
    let stale_state = Arc::clone(&snapshot.state);
    resize_grid_batched(
        state,
        mode_stack,
        move || stale_state.album_grid.build_seq.load(Relaxed) != snapshot.build_seq,
        resolve_cards,
        dispatch_covers,
    )
}

/// Resolve a single card's cover from the grid's cover snapshot, if it has
/// one. Applied during a batched zoom resize after the card's geometry pass.
fn resolve_cover_at(
    idx_to_album: &HashMap<usize, i64>,
    cover_cache: &CoverArtCache,
    idx: usize,
    overlay: &Overlay,
    size: i32,
) {
    if let Some(&album_id) = idx_to_album.get(&idx) {
        resolve_cover_widget(cover_cache, overlay, album_id, size);
    }
}

/// Populate a batch of album cards into the flow box.
///
/// Bails out if `build_seq` no longer matches the current build generation
/// (the build was superseded by a rebuild or refresh). On the final batch,
/// dispatches cover decoding, snapshots the cover metadata for in-place zoom
/// resizing, and marks the grid ready. Returns `Break` when done or stale.
fn fill_album_grid(
    cached: &CachedAlbumData,
    remaining: &mut Vec<usize>,
    overlays: &mut Vec<Overlay>,
    cover_art_data: &mut Vec<(i64, usize, String)>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    build_seq: u64,
) -> ControlFlow {
    if fill_grid_batch(
        state.album_grid.build_seq.load(Relaxed) == build_seq,
        state,
        remaining,
        |idx, size| {
            let album = &cached.albums[idx];
            let index = overlays.len();
            let artist_name = cached
                .artist_names
                .get(&album.artist_id)
                .map_or("Unknown Artist", String::as_str);
            let fi = cached
                .format_info
                .get(&album.id)
                .cloned()
                .unwrap_or_default();
            let (card, overlay) = build_album_card(state, album, artist_name, &fi, size);
            if let Some(path) = &album.artwork_path {
                cover_art_data.push((album.id, index, path.clone()));
            }
            overlays.push(overlay);
            flow.append(&card.upcast::<Widget>());
        },
    ) == Continue
    {
        return Continue;
    }
    if state.album_grid.build_seq.load(Relaxed) != build_seq {
        return Break;
    }
    let size = grid_cover_size(state.storage.get_grid_zoom_level());
    load_cover_art_async(
        state,
        cover_art_data,
        overlays,
        &state.cover_art_cache,
        size,
    );
    *state.album_grid_covers.lock() = Arc::new(take(cover_art_data));
    state.album_grid.ready.store(true, Relaxed);
    Break
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

            let album_ids: Vec<i64> = indices.iter().map(|&i| cached.albums[i].id).collect();
            setup_flowbox_keyboard_nav(&flow, state, album_ids, AlbumDetail);

            let cached_owned = CachedAlbumData {
                albums: Arc::clone(&cached.albums),
                artist_names: Arc::clone(&cached.artist_names),
                format_info: Arc::clone(&cached.format_info),
            };
            let state = Arc::clone(state);
            let build_seq = state
                .album_grid
                .build_seq
                .fetch_add(1, Relaxed)
                .wrapping_add(1);
            let mut overlays: Vec<Overlay> = Vec::new();
            let mut cover_art_data: Vec<(i64, usize, String)> = Vec::new();
            let mut remaining: Vec<usize> = indices.iter().rev().copied().collect();

            idle_add_local(move || {
                fill_album_grid(
                    &cached_owned,
                    &mut remaining,
                    &mut overlays,
                    &mut cover_art_data,
                    &flow,
                    &state,
                    build_seq,
                )
            });
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

    let my_gen = state.album_grid.generation.load(Relaxed);
    let (albums_res, artist_names_res) = join!(
        state.storage.get_all_albums(),
        state.storage.get_all_artists(),
    );

    let albums = match albums_res {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, "Failed to load albums for lazy build");
            return;
        }
    };

    if albums.is_empty() {
        if state.album_grid.generation.load(Relaxed) != my_gen {
            return;
        }
        if stack.child_by_name("grid").is_some() {
            stack.set_visible_child_name("grid");
            return;
        }
        *state.album_grid.cache.lock() = Some(CachedAlbumData {
            albums: Arc::new(albums),
            artist_names: Arc::new(HashMap::new()),
            format_info: Arc::new(HashMap::new()),
        });
        show_albums_empty(state, stack);
        return;
    }

    let album_ids: Vec<i64> = albums.iter().map(|a| a.id).collect();
    let format_info = state
        .storage
        .get_albums_format_info(&album_ids)
        .await
        .unwrap_or_default();

    if state.album_grid.generation.load(Relaxed) != my_gen {
        return;
    }
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    let artist_names: HashMap<i64, String> = match artist_names_res {
        Ok(artists) => artists.into_iter().map(|a| (a.id, a.name)).collect(),
        Err(e) => {
            warn!(error = %e, "Failed to load artists for lazy build");
            HashMap::new()
        }
    };

    let cached = CachedAlbumData {
        albums: Arc::new(albums),
        artist_names: Arc::new(artist_names),
        format_info: Arc::new(format_info),
    };
    let indices = album_sort_indices(state, &cached);
    build_album_mode(state, stack, narrow_state, mode, &cached, &indices);
    *state.album_grid.cache.lock() = Some(cached);
    stack.set_visible_child_name(child_name);
}

/// Show the empty‑albums state widget, replacing any existing grid child.
fn show_albums_empty(state: &Arc<AppState>, stack: &Stack) {
    state.album_grid.ready.store(false, Relaxed);
    *state.album_grid_covers.lock() = Arc::new(Vec::new());
    if stack.child_by_name("grid").is_none() {
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
    }
    stack.set_visible_child_name("grid");
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        storage::sort_rules::{
            AlbumSortCriteria::{Artist, Title},
            AlbumSortItem,
            SortOrder::{Ascending, Descending},
        },
        ui::library::albums::{Album, sorted_album_indices},
    };

    fn mock_album(id: i64, title: &str, artist_id: i64) -> Album {
        Album {
            id,
            title: title.into(),
            artist_id,
            year: Some(2020),
            genre: None,
            artwork_path: None,
            track_count: 10,
            total_duration: 300.0,
            format_summary: String::new(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96000),
        }
    }

    #[test]
    fn sorted_album_indices_orders_by_priority_items() {
        let albums = vec![
            mock_album(1, "Beta", 10),
            mock_album(2, "Alpha", 10),
            mock_album(3, "Gamma", 20),
        ];
        let mut artist_names = HashMap::new();
        artist_names.insert(10, "Zed".to_string());
        artist_names.insert(20, "Adam".to_string());

        let items = vec![
            AlbumSortItem {
                criteria: Artist,
                order: Ascending,
            },
            AlbumSortItem {
                criteria: Title,
                order: Descending,
            },
        ];

        let indices = sorted_album_indices(&albums, &artist_names, &items);
        assert_eq!(indices, vec![2, 0, 1]);
    }

    #[test]
    fn sorted_album_indices_defaults_to_ascending_title() {
        let albums = vec![
            mock_album(1, "Beta", 10),
            mock_album(2, "Alpha", 10),
            mock_album(3, "Gamma", 10),
        ];
        let artist_names = HashMap::new();
        let items = vec![AlbumSortItem {
            criteria: Title,
            order: Descending,
        }];

        let indices = sorted_album_indices(&albums, &artist_names, &items);
        assert_eq!(indices, vec![2, 0, 1]);
    }
}

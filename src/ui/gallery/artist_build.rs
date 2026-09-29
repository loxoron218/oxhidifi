//! Artist grid/column construction and lazy builds.
//!
//! Builds the artist grid (`FlowBox`) and column (`ColumnView`) views from
//! cached data and fetches library data on first use. Orchestration (sort/zoom
//! reactions, previews, resizes) lives in [`artist_grid`](super::artist_grid).

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
        },
        gtk::{Box, FlowBox, Orientation::Vertical, Overlay, Stack, Widget},
        prelude::{BoxExt, Cast},
    },
    tracing::warn,
};

use crate::{
    app::runtime::{AppState, CachedArtistData, NavigationEvent::ArtistDetail},
    storage::{
        Storage,
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::{
        gallery::{
            avatar::build_artist_card,
            empty::{add_scrolled, show_library_empty},
            grid_batch::fill_grid_batch,
            grid_flow::build_grid,
            keyboard_nav::setup_flowbox_keyboard_nav,
            narrow_flag::NarrowState,
            precedence::artist_sort_indices,
            stack_cache::{
                GridBuildTarget, mode_child_name, show_mode_child_if_built, spawn_grid_fetch,
            },
            table::build_artist_column_view,
        },
        zoom::effective_grid_cover_size,
    },
};

/// Populate a batch of artist cards into the flow box.
///
/// Bails out if `build_seq` no longer matches the current build generation
/// (the build was superseded by a rebuild or refresh). On the final batch,
/// marks the grid ready for in-place zoom resizing. Returns `Break` when
/// done or stale.
///
/// # Arguments
///
/// * `cover_size` - Avatar size to build cards at (already resolved for narrow windows by the
///   caller).
fn fill_artist_grid(
    cached: &CachedArtistData,
    remaining: &mut Vec<usize>,
    overlays: &mut Vec<Overlay>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    build_seq: u64,
    cover_size: i32,
) -> ControlFlow {
    if fill_grid_batch(
        state.artist_grid.build_seq.load(Relaxed) == build_seq,
        cover_size,
        remaining,
        |idx, size| {
            let Some(artist) = cached.artists.get(idx) else {
                return;
            };
            let (card, overlay) = build_artist_card(state, artist, size);
            overlays.push(overlay);
            flow.append(&card.upcast::<Widget>());
        },
    ) == Continue
    {
        return Continue;
    }
    if state.artist_grid.build_seq.load(Relaxed) != build_seq {
        return Break;
    }
    state.artist_grid.ready.store(true, Relaxed);
    Break
}

/// Build the given `mode` view (grid or column) and add it to `stack`.
///
/// Borrows the shared artist data from `cached` and derives the display order
/// from `indices`, so no deep clone is performed. Each mode is wrapped in
/// its own `ScrolledWindow` so scroll positions are kept independent.
///
/// # Arguments
///
/// * `cached` - Shared artist data
/// * `indices` - Display order of `cached.artists` (e.g. a sorted index vector)
/// * `narrow_state` - Narrow-width tracker for the initial avatar size
fn build_artist_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &NarrowState,
    mode: ViewMode,
    cached: &CachedArtistData,
    indices: &[usize],
) {
    let cover_size =
        effective_grid_cover_size(state.storage.get_grid_zoom_level(), narrow_state.get());
    match mode {
        Grid => {
            state.artist_grid.ready.store(false, Relaxed);
            let grid_container = Box::builder().orientation(Vertical).build();
            let flow = build_grid(
                "Artist library grid \u{2014} click an artist to view albums",
                cover_size,
            );
            grid_container.append(&flow);
            add_scrolled(stack, &grid_container, "grid");

            let artist_ids: Vec<i64> = indices
                .iter()
                .filter_map(|&i| cached.artists.get(i).map(|artist| artist.id))
                .collect();
            setup_flowbox_keyboard_nav(&flow, state, artist_ids, ArtistDetail);

            let cached_owned = CachedArtistData {
                artists: Arc::clone(&cached.artists),
            };
            let state_fill = Arc::clone(state);
            let build_seq = state_fill
                .artist_grid
                .build_seq
                .fetch_add(1, Relaxed)
                .wrapping_add(1);
            let mut overlays: Vec<Overlay> = Vec::new();
            let mut remaining: Vec<usize> = indices.iter().rev().copied().collect();

            state.handles.lock().retain_source(idle_add_local(move || {
                fill_artist_grid(
                    &cached_owned,
                    &mut remaining,
                    &mut overlays,
                    &flow,
                    &state_fill,
                    build_seq,
                    cover_size,
                )
            }));
        }
        Column => {
            let column_view = build_artist_column_view(state, cached, indices);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Show the empty‑artists state widget, replacing any existing grid child.
///
/// Two-state per FR-007: "No Music Library Configured" when no directories
/// are configured, "No Music Found" when directories exist but contain no
/// artists.
fn show_artists_empty(state: &Arc<AppState>, stack: &Stack) {
    state.artist_grid.ready.store(false, Relaxed);
    show_library_empty(state, stack, "avatar-default-symbolic", "Artist icon");
}

/// Lazily build a view mode that wasn't constructed at startup.
///
/// Re‑fetches data from storage, builds the requested `mode` widget,
/// adds it to `stack`, and switches to it.  No‑op if the child already
/// exists (race‑guard).  The synchronous front half runs the race‑guard
/// and cached‑build checks; the storage re‑fetch and widget construction
/// run in a local future on the `GLib` main context.
///
/// # Arguments
///
/// * `narrow_state` - Narrow-width tracker for the initial avatar size.
pub fn lazy_build_artist_mode(
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
        &state.artist_grid.cache,
        |cached: &CachedArtistData| cached.artists.is_empty(),
        |cached| {
            let indices = artist_sort_indices(state, cached);
            build_artist_mode(state, stack, narrow_state, mode, cached, &indices);
        },
        || show_artists_empty(state, stack),
        async move |fetch_state: Arc<AppState>,
                    fetch_stack: Stack,
                    fetch_narrow: Arc<NarrowState>,
                    fetch_mode: ViewMode| {
            let my_gen = fetch_state.artist_grid.generation.load(Relaxed);
            let artists = match fetch_state.storage.get_all_artists().await {
                Ok(a) => a
                    .into_iter()
                    .filter(|a| a.album_count > 0)
                    .collect::<Vec<_>>(),
                Err(e) => {
                    warn!(error = %e, "Failed to load artists for lazy build");
                    return;
                }
            };

            if fetch_state.artist_grid.generation.load(Relaxed) != my_gen {
                return;
            }
            if show_mode_child_if_built(&fetch_stack, fetch_mode) {
                return;
            }

            if artists.is_empty() {
                *fetch_state.artist_grid.cache.lock() = Some(CachedArtistData {
                    artists: Arc::new(artists),
                });
                show_artists_empty(&fetch_state, &fetch_stack);
                return;
            }

            let cached = CachedArtistData {
                artists: Arc::new(artists),
            };
            let indices = artist_sort_indices(&fetch_state, &cached);
            build_artist_mode(
                &fetch_state,
                &fetch_stack,
                &fetch_narrow,
                fetch_mode,
                &cached,
                &indices,
            );
            *fetch_state.artist_grid.cache.lock() = Some(cached);
            fetch_stack.set_visible_child_name(mode_child_name(fetch_mode));
        },
    );
}

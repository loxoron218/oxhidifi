//! Artist grid/column view.
//!
//! Displays artists in a responsive `FlowBox` grid or sortable
//! `GtkColumnView`. Both views are built once and held in a
//! `GtkStack` — switching between them toggles visibility without
//! any data re‑fetch or widget reconstruction.

use std::sync::{Arc, atomic::Ordering::Relaxed};

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
    tracing::warn,
};

use crate::{
    app::runtime::{AppState, CachedArtistData, NavigationEvent::ArtistDetail},
    storage::{
        Storage,
        active_tab::ActiveTab::Artists,
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::{
        gallery::{
            avatar::build_artist_card,
            coalescer::spawn_listen_sort_zoom,
            empty::{
                EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
            },
            grid_flow::{build_grid, fill_grid_batch, resize_grid_batched},
            keyboard_nav::setup_flowbox_keyboard_nav,
            narrow_flag::NarrowState,
            precedence::artist_sort_indices,
            rebuild_debounce::{
                RebuildAction::{DeferDirty, Resize},
                decide_rebuild,
            },
            stack_cache::{clear_mode_children, take_stale_mode_child, try_build_from_cache},
            table::build_artist_column_view,
        },
        zoom::grid_cover_size,
    },
};

/// Build the artist grid view.
///
/// Creates a `LibraryGrid` that holds both grid (`FlowBox`) and column
/// (`ColumnView`) layouts in a `Stack`.  Data is fetched once; switching
/// between modes is a fast `set_visible_child_name` call.
///
/// # Arguments
///
/// * `state` - Application state
/// * `narrow_mode` - Narrow‑mode tracker for adaptive column hiding
pub fn build_artist_grid(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> LibraryGrid {
    let nm = Arc::clone(narrow_state);
    let populate =
        |stack: &Stack, state: Arc<AppState>, _: Arc<NarrowState>, initial_mode: ViewMode| {
            lazy_build_artist_mode(&state, stack, initial_mode);
        };
    let lg = build_library_grid(state, &nm, populate);

    let grid_state = Arc::clone(state);
    let grid_stack = lg.mode_stack.clone();
    let preview_state = Arc::clone(state);
    let preview_stack = lg.mode_stack.clone();
    spawn_listen_sort_zoom(
        state,
        state.artists_sort_rx.clone(),
        state.artists_zoom_rx.clone(),
        move || {
            if preview_state.active_tab.borrow() == Artists
                && preview_state.view_mode.borrow() == Grid
                && preview_state.artist_grid.ready.load(Relaxed)
            {
                resize_artist_grid(&preview_state, &preview_stack);
            }
        },
        move |sort_fired, zoom_fired| {
            rebuild_artist_current_mode(&grid_state, &grid_stack, sort_fired, zoom_fired);
        },
    );

    lg
}

/// Rebuild or resize the artist grid after a sort/zoom change.
///
/// Zoom-only changes resize the existing cards in place (no widget churn,
/// scroll position preserved). Sort changes — or any state that invalidates
/// the current cards — fall back to a full rebuild from the in-memory cache.
/// When the tab is hidden, marks the grid dirty so it is rebuilt on switch.
fn rebuild_artist_current_mode(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    sort_fired: bool,
    zoom_fired: bool,
) {
    let action = decide_rebuild(
        state.active_tab.borrow(),
        Artists,
        state.view_mode.borrow(),
        sort_fired,
        zoom_fired,
        state.artist_grid.ready.load(Relaxed),
    );
    if action == DeferDirty {
        state.artist_grid.dirty.store(true, Relaxed);
        return;
    }
    if action == Resize && resize_artist_grid(state, mode_stack) {
        return;
    }
    let Some(mode) = take_stale_mode_child(state, Artists, mode_stack) else {
        state.artist_grid.dirty.store(true, Relaxed);
        return;
    };
    if sort_fired {
        clear_mode_children(mode_stack);
    }
    state.artist_grid.dirty.store(false, Relaxed);
    lazy_build_artist_mode(state, mode_stack, mode);
}

/// Resize the live artist grid's cards to the current zoom level in place.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) that
/// updates the `FlowBox` spacing and every card's avatar size across idle
/// callbacks, so a zoom on a large library does not block the frame clock.
/// Artists have no per-size cover art to decode, so no cover dispatch.
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
fn resize_artist_grid(state: &Arc<AppState>, mode_stack: &Stack) -> bool {
    let build_seq = state.artist_grid.build_seq.load(Relaxed);
    let stale_state = Arc::clone(state);
    resize_grid_batched(
        state,
        mode_stack,
        move || stale_state.artist_grid.build_seq.load(Relaxed) != build_seq,
        |_, _, _| {},
        |_, _| {},
    )
}

/// Populate a batch of artist cards into the flow box.
///
/// Bails out if `build_seq` no longer matches the current build generation
/// (the build was superseded by a rebuild or refresh). On the final batch,
/// marks the grid ready for in-place zoom resizing. Returns `Break` when
/// done or stale.
fn fill_artist_grid(
    cached: &CachedArtistData,
    remaining: &mut Vec<usize>,
    overlays: &mut Vec<Overlay>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    build_seq: u64,
) -> ControlFlow {
    if fill_grid_batch(
        state.artist_grid.build_seq.load(Relaxed) == build_seq,
        state,
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
fn build_artist_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    mode: ViewMode,
    cached: &CachedArtistData,
    indices: &[usize],
) {
    let cover_size = grid_cover_size(state.storage.get_grid_zoom_level());
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
            let state = Arc::clone(state);
            let build_seq = state
                .artist_grid
                .build_seq
                .fetch_add(1, Relaxed)
                .wrapping_add(1);
            let mut overlays: Vec<Overlay> = Vec::new();
            let mut remaining: Vec<usize> = indices.iter().rev().copied().collect();

            idle_add_local(move || {
                fill_artist_grid(
                    &cached_owned,
                    &mut remaining,
                    &mut overlays,
                    &flow,
                    &state,
                    build_seq,
                )
            });
        }
        Column => {
            let column_view = build_artist_column_view(state, cached, indices);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Show the empty‑artists state widget, replacing any existing grid child.
fn show_artists_empty(state: &Arc<AppState>, stack: &Stack) {
    state.artist_grid.ready.store(false, Relaxed);
    let empty = build_empty_state(
        state,
        &EmptyStateParams {
            icon_name: "avatar-default-symbolic",
            icon_label: "Artist icon",
            heading: "No Artists Found",
            heading_label: "No artists found",
            description: "Add a music folder to see your artists here.",
            description_label: "Add a music folder to see your artists here.",
        },
    );
    if stack.child_by_name("grid").is_none() {
        stack.add_named(&empty, Some("grid"));
    }
    stack.set_visible_child_name("grid");
}

/// Lazily build a view mode that wasn't constructed at startup.
///
/// Re‑fetches data from storage, builds the requested `mode` widget,
/// adds it to `stack`, and switches to it.  No‑op if the child already
/// exists (race‑guard).  The synchronous front half runs the race‑guard
/// and cached‑build checks; the storage re‑fetch and widget construction
/// run in a local future on the `GLib` main context.
pub fn lazy_build_artist_mode(state: &Arc<AppState>, stack: &Stack, mode: ViewMode) {
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    if try_build_from_cache(
        &state.artist_grid.cache,
        stack,
        child_name,
        |cached| cached.artists.is_empty(),
        |cached| {
            let indices = artist_sort_indices(state, cached);
            build_artist_mode(state, stack, mode, cached, &indices);
        },
        || show_artists_empty(state, stack),
    ) {
        return;
    }

    let state = Arc::clone(state);
    let stack = stack.clone();
    spawn_future_local(async move {
        let my_gen = state.artist_grid.generation.load(Relaxed);
        let artists = match state.storage.get_all_artists().await {
            Ok(a) => a
                .into_iter()
                .filter(|a| a.album_count > 0)
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!(error = %e, "Failed to load artists for lazy build");
                return;
            }
        };

        if state.artist_grid.generation.load(Relaxed) != my_gen {
            return;
        }
        if stack.child_by_name(child_name).is_some() {
            stack.set_visible_child_name(child_name);
            return;
        }

        if artists.is_empty() {
            *state.artist_grid.cache.lock() = Some(CachedArtistData {
                artists: Arc::new(artists),
            });
            show_artists_empty(&state, &stack);
            return;
        }

        let cached = CachedArtistData {
            artists: Arc::new(artists),
        };
        let indices = artist_sort_indices(&state, &cached);
        build_artist_mode(&state, &stack, mode, &cached, &indices);
        *state.artist_grid.cache.lock() = Some(cached);
        stack.set_visible_child_name(child_name);
    });
}

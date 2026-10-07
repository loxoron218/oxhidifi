//! Artist grid/column view.
//!
//! Displays artists in a responsive `FlowBox` grid or sortable
//! `GtkColumnView`. Both views are built once and held in a
//! `GtkStack` — switching between them toggles visibility without
//! any data re‑fetch or widget reconstruction.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {libadwaita::gtk::Stack, tracing::warn};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::Artists,
        view_mode::ViewMode::{self, Grid},
    },
    ui::{
        gallery::{
            artist_build::lazy_build_artist_mode,
            coalescer::spawn_grid_sort_zoom,
            empty::{LibraryGrid, build_library_grid},
            grid_batch::resize_grid_batched,
            grid_fit::{grid_is_current, upgrade_hidden_resize},
            label_sizing::resize_artist_card,
            narrow_flag::NarrowState,
            rebuild_debounce::{
                RebuildAction::{DeferDirty, Resize},
                decide_rebuild,
            },
            stack_cache::{clear_mode_children, take_stale_mode_child},
        },
        zoom::effective_grid_cover_size,
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
        |stack: &Stack, state: Arc<AppState>, narrow: Arc<NarrowState>, initial_mode: ViewMode| {
            lazy_build_artist_mode(&state, stack, &narrow, initial_mode);
        };
    let lg = build_library_grid(state, &nm, populate);

    spawn_grid_sort_zoom(
        state,
        &lg.mode_stack,
        narrow_state,
        state.artists_sort_rx.clone(),
        state.artists_zoom_rx.clone(),
        preview_artist_resize,
        rebuild_artist_current_mode,
    );

    lg
}

/// Resolve the artist avatar size to render for the current narrow state.
///
/// Mirrors the album grid's narrow snap so both grids stay visually in sync.
///
/// # Arguments
///
/// * `state` - Application state (stored zoom level source).
/// * `narrow_state` - Narrow-width tracker.
///
/// # Returns
///
/// Avatar size in pixels to render.
fn effective_artist_avatar(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> i32 {
    effective_grid_cover_size(state.storage.get_grid_zoom_level(), narrow_state.get())
}

/// Preview a zoom or narrow-window change on the live artist grid.
///
/// Resizes avatars immediately when the grid is visible and ready; skips
/// grids already rendering at the effective size.
fn preview_artist_resize(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    if state.active_tab.borrow() != Artists
        || state.view_mode.borrow() != Grid
        || !state.artist_grid.ready.load(Relaxed)
    {
        return;
    }
    let size = effective_artist_avatar(state, narrow_state);
    if !grid_is_current(mode_stack, size) {
        _ = resize_artist_grid(state, mode_stack, size);
    }
}

/// Rebuild or resize the artist grid after a sort/zoom change.
///
/// Zoom-only changes resize the existing cards in place (no widget churn,
/// scroll position preserved) — including on hidden tabs, so a stale
/// oversized page never forces the window past its minimum. Sort changes — or
/// any state that invalidates the current cards — fall back to a full rebuild
/// from the in-memory cache. When the tab is hidden, marks the grid dirty so
/// it is rebuilt on switch. Narrow-window changes arrive here as zoom
/// notifications (see `wire_narrow_fit`) and take the same in-place path.
fn rebuild_artist_current_mode(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    sort_fired: bool,
    zoom_fired: bool,
) {
    let action = upgrade_hidden_resize(
        decide_rebuild(
            state.active_tab.borrow(),
            Artists,
            state.view_mode.borrow(),
            sort_fired,
            zoom_fired,
            state.artist_grid.ready.load(Relaxed),
        ),
        sort_fired,
        zoom_fired,
        state.view_mode.borrow(),
        state.artist_grid.ready.load(Relaxed),
        mode_stack,
    );
    if action == DeferDirty {
        state.artist_grid.dirty.store(true, Relaxed);
        return;
    }
    if action == Resize {
        let size = effective_artist_avatar(state, narrow_state);
        if grid_is_current(mode_stack, size) {
            return;
        }
        if resize_artist_grid(state, mode_stack, size) {
            return;
        }
    }
    let Some(mode) = take_stale_mode_child(state, Artists, mode_stack) else {
        state.artist_grid.dirty.store(true, Relaxed);
        return;
    };
    if sort_fired {
        clear_mode_children(mode_stack);
    }
    state.artist_grid.dirty.store(false, Relaxed);
    lazy_build_artist_mode(state, mode_stack, narrow_state, mode);
}

/// Resize the live artist grid's cards to the given cover size in place.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) that
/// updates the `FlowBox` spacing and every card's avatar size plus label caps
/// across idle callbacks, so a zoom on a large library does not block the
/// frame clock. Artists have no per-size cover art to decode, so no cover dispatch.
///
/// # Arguments
///
/// * `cover_size` - Avatar size to resize to (already resolved for narrow windows by the caller).
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
fn resize_artist_grid(state: &Arc<AppState>, mode_stack: &Stack, cover_size: i32) -> bool {
    let build_seq = state.artist_grid.build_seq.load(Relaxed);
    let zoom_seq = state.artist_grid.zoom_seq.load(Relaxed);
    let stale_state = Arc::clone(state);
    resize_grid_batched(
        state,
        mode_stack,
        cover_size,
        move || {
            stale_state.artist_grid.build_seq.load(Relaxed) != build_seq
                || stale_state.artist_grid.zoom_seq.load(Relaxed) != zoom_seq
        },
        |_, overlay, size| {
            if let Err(e) = resize_artist_card(overlay, size) {
                warn!(error = %e, size, "Skipping artist card resize");
            }
        },
        |_, _| {},
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering::Relaxed};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::MainContext,
            gtk::{self, FlowBox, Stack, test},
        },
    };

    use crate::{
        app::{mocks::pump_in_test_runtime, runtime::AppState},
        storage::catalog::Artist,
        ui::gallery::{
            artist_grid::rebuild_artist_current_mode, avatar::build_artist_card,
            empty::add_scrolled, grid_fit::grid_is_current, grid_flow::build_grid,
            narrow_flag::NarrowState,
        },
    };

    fn pump_main_context() {
        let mut iterations: usize = 0;
        while MainContext::default().iteration(false) && iterations < 1000 {
            iterations = iterations.saturating_add(1);
        }
    }

    #[test]
    fn hidden_artist_grid_resizes_on_zoom_only_change() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let stack = Stack::new();
        let flow = build_grid("artist grid", 240);
        let artist = Artist {
            id: 1,
            name: "Test Artist".into(),
            album_count: 3,
        };
        let (card, _) = build_artist_card(&state, &artist, 240);
        FlowBox::append(&flow, &card);
        add_scrolled(&stack, &flow, "grid");
        state.artist_grid.ready.store(true, Relaxed);
        let narrow = NarrowState::new_shared();
        narrow.set(true);
        rebuild_artist_current_mode(&state, &stack, &narrow, false, true);
        pump_in_test_runtime(pump_main_context)?;
        ensure!(
            grid_is_current(&stack, 120),
            "a zoom-only change must shrink a built hidden grid in place"
        );
        Ok(())
    }
}

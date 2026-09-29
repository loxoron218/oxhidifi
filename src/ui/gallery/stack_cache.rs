//! Mode-stack child caching and invalidation for library grids.

use std::sync::Arc;

use {
    libadwaita::{glib::spawn_future_local, gtk::Stack},
    parking_lot::Mutex,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab,
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::gallery::narrow_flag::NarrowState,
};

/// Build targets for a lazily constructed library mode.
///
/// Bundles the concrete arguments shared by the album/artist lazy builds so
/// [`spawn_grid_fetch`] stays under the argument limit.
#[derive(Clone, Copy, Debug)]
pub struct GridBuildTarget<'a> {
    /// Application state owning task handles and storage.
    pub state: &'a Arc<AppState>,
    /// Mode stack receiving the built child.
    pub stack: &'a Stack,
    /// Narrow-width tracker forwarded to builds and fetches.
    pub narrow_state: &'a Arc<NarrowState>,
    /// Grid or column mode to build.
    pub mode: ViewMode,
}

/// Remove the stale child for the current view mode if `tab` is the active tab.
///
/// # Arguments
///
/// * `state` - App state used to check the active tab and current view mode
/// * `tab` - The library tab that must be active for the rebuild
/// * `mode_stack` - The mode stack to remove the stale child from
///
/// # Returns
///
/// The current `ViewMode` if `tab` is active, otherwise `None`.
pub fn take_stale_mode_child(
    state: &Arc<AppState>,
    tab: ActiveTab,
    mode_stack: &Stack,
) -> Option<ViewMode> {
    if state.active_tab.borrow() != tab {
        return None;
    }
    let mode = state.view_mode.borrow();
    let child_name = mode_child_name(mode);
    if let Some(child) = mode_stack.child_by_name(child_name) {
        mode_stack.remove(&child);
    }
    Some(mode)
}

/// Remove both grid and column mode children, so the next mode switch
/// rebuilds them lazily from the in-memory cache.
///
/// Used on sort changes, where every mode child reflects the previous sort
/// order and must be invalidated.
pub fn clear_mode_children(mode_stack: &Stack) {
    for name in ["grid", "column"] {
        if let Some(child) = mode_stack.child_by_name(name) {
            mode_stack.remove(&child);
        }
    }
}

/// Try to build a library mode from its in-memory cache.
///
/// Returns `true` when the cache was used (including the empty‑state case),
/// meaning the caller should return. The empty check, build, and empty‑state
/// presentation are provided by the caller so the album and artist grids
/// share this single flow.
pub fn try_build_from_cache<T>(
    cache: &Mutex<Option<T>>,
    stack: &Stack,
    child_name: &str,
    is_empty: impl Fn(&T) -> bool,
    build: impl FnOnce(&T),
    show_empty: impl FnOnce(),
) -> bool {
    let guard = cache.lock();
    let Some(cached) = guard.as_ref() else {
        return false;
    };
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return true;
    }
    if is_empty(cached) {
        drop(guard);
        show_empty();
    } else {
        build(cached);
        drop(guard);
        stack.set_visible_child_name(child_name);
    }
    true
}

/// Show the mode-stack child for `mode` when already built.
///
/// Shared race-guard for lazy mode builds: showing the existing child is a
/// no-op for the caller, so only a missing child falls through to a build.
///
/// # Arguments
///
/// * `stack` - Mode stack holding the `"grid"`/`"column"` children.
/// * `mode` - Grid or column mode to show.
///
/// # Returns
///
/// `true` when the child existed (and was shown), `false` when the caller
/// must build it.
#[must_use]
pub fn show_mode_child_if_built(stack: &Stack, mode: ViewMode) -> bool {
    let child_name = mode_child_name(mode);
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return true;
    }
    false
}

/// Name of the mode-stack child for `mode` (`"grid"` or `"column"`).
///
/// # Arguments
///
/// * `mode` - Grid or column mode.
///
/// # Returns
///
/// The child name to look up on the mode stack.
#[must_use]
pub const fn mode_child_name(mode: ViewMode) -> &'static str {
    match mode {
        Grid => "grid",
        Column => "column",
    }
}

/// Lazily build a library mode, fetching data only when uncached.
///
/// Shared shell for the album/artist lazy builds: shows the existing child,
/// builds from the in-memory cache, or spawns the grid-specific `fetch` on
/// the main context. Clones the fetch inputs up front so the two grids' lazy
/// entry points stay one call each. Keeping this sequence in one place keeps
/// the preload behavior from drifting apart.
///
/// # Arguments
///
/// * `target` - Build targets (state, stack, narrow flag, mode).
/// * `cache` - In-memory data cache consulted before fetching.
/// * `is_empty` - Reports whether cached data has nothing to show.
/// * `build` - Builds the mode widget from cached data.
/// * `show_empty` - Shows the empty state for empty cached data.
/// * `fetch` - Fetches fresh data and completes the build; runs on the `GLib` main context and must
///   handle its own staleness guards.
pub fn spawn_grid_fetch<T, I, B, E, F>(
    target: GridBuildTarget<'_>,
    cache: &Mutex<Option<T>>,
    is_empty: I,
    build: B,
    show_empty: E,
    fetch: F,
) where
    I: Fn(&T) -> bool,
    B: FnOnce(&T),
    E: FnOnce(),
    F: AsyncFnOnce(Arc<AppState>, Stack, Arc<NarrowState>, ViewMode) + 'static,
{
    if show_mode_child_if_built(target.stack, target.mode) {
        return;
    }
    let child_name = mode_child_name(target.mode);
    if try_build_from_cache(cache, target.stack, child_name, is_empty, build, show_empty) {
        return;
    }
    let fetch_state = Arc::clone(target.state);
    let fetch_stack = target.stack.clone();
    let fetch_narrow = Arc::clone(target.narrow_state);
    let mode = target.mode;
    target
        .state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            fetch(fetch_state, fetch_stack, fetch_narrow, mode).await;
        }));
}

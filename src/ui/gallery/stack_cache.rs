//! Mode-stack child caching and invalidation for library grids.

use std::sync::Arc;

use {libadwaita::gtk::Stack, parking_lot::Mutex};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab,
        view_mode::ViewMode::{self, Column, Grid},
    },
};

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
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
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

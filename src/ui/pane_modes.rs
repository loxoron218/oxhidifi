//! Library mode switching for pane stacks.
//!
//! Grid/column switching for the active library tab, building views lazily so
//! hidden tabs never construct widgets the user is not looking at.

use std::sync::Arc;

use libadwaita::gtk::Stack;

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{Albums, Artists, Signal},
        view_mode::ViewMode,
    },
    ui::gallery::{
        album_grid::lazy_build_album_mode, artist_build::lazy_build_artist_mode,
        narrow_flag::NarrowState, stack_cache::mode_child_name,
    },
};

/// Switch the currently active tab's mode stack to `mode`, building the view lazily if needed.
///
/// Hidden tabs are skipped — they reconcile their mode when activated via
/// [`crate::ui::switching::handle_tab_switch`], so toggling modes never builds
/// a view the user is not looking at.
///
/// # Arguments
///
/// * `state` - Application state owning the active tab.
/// * `mode` - Grid or column mode to show.
/// * `album_stack` - Mode stack for the albums tab.
/// * `artist_stack` - Mode stack for the artists tab.
/// * `narrow_state` - Shared narrow-mode flag for grid layout.
pub fn switch_mode_for_active_tab(
    state: &Arc<AppState>,
    mode: ViewMode,
    album_stack: &Stack,
    artist_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    let (stack, name) = match state.active_tab.borrow() {
        Albums => (album_stack, "albums"),
        Artists => (artist_stack, "artists"),
        Signal => return,
    };
    switch_mode_for_stack(state, name, stack, narrow_state, mode);
}

/// Switch the given tab's mode-stack to `mode`, building the view lazily if needed.
///
/// # Arguments
///
/// * `state` - Application state for lazy view construction.
/// * `tab` - Tab name (`albums` or `artists`; others are ignored).
/// * `stack` - Mode stack to switch.
/// * `narrow_state` - Shared narrow-mode flag for grid layout.
/// * `mode` - Grid or column mode to show.
pub fn switch_mode_for_stack(
    state: &Arc<AppState>,
    tab: &str,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    mode: ViewMode,
) {
    let child = mode_child_name(mode);
    if stack.child_by_name(child).is_none() {
        match tab {
            "albums" => lazy_build_album_mode(state, stack, narrow_state, mode),
            "artists" => lazy_build_artist_mode(state, stack, narrow_state, mode),
            _ => {}
        }
    }
    stack.set_visible_child_name(child);
}

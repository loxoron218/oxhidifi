//! Popover zoom button sensitivity sync.
//!
//! The zoom buttons live in the view-toggle popover, but the zoom level can
//! also change via `Ctrl+`/`Ctrl-` key bindings and the zoom range changes
//! with the view mode (grid and column have different limits). The popover
//! refreshes its own buttons on show and on view-mode switches using local
//! clones, so no shared widget state is needed: widgets stay on the GTK main
//! thread and key-driven changes are picked up on the next popover open.

use libadwaita::{gtk::Button, prelude::WidgetExt};

use crate::{
    app::runtime::AppState,
    storage::view_mode::ViewMode::{Column, Grid},
    ui::zoom::{GRID_ZOOM_MAX, GRID_ZOOM_MIN, LIST_ZOOM_MAX, LIST_ZOOM_MIN},
};

/// Update zoom button sensitivity for the current view mode and zoom level.
///
/// Disables Zoom-Out at the minimum and Zoom-In at the maximum so the
/// popover can never offer a clamped no-op zoom.
///
/// # Arguments
///
/// * `state` - Application state (view mode and zoom level source).
/// * `zoom_out_btn` - Button to disable at the minimum zoom.
/// * `zoom_in_btn` - Button to disable at the maximum zoom.
pub fn update_zoom_sensitivity(state: &AppState, zoom_out_btn: &Button, zoom_in_btn: &Button) {
    let mode = state.storage.get_view_mode();
    let (min_zoom, max_zoom, current_zoom) = match mode {
        Grid => (
            GRID_ZOOM_MIN,
            GRID_ZOOM_MAX,
            state.storage.get_grid_zoom_level(),
        ),
        Column => (
            LIST_ZOOM_MIN,
            LIST_ZOOM_MAX,
            state.storage.get_list_zoom_level(),
        ),
    };
    zoom_out_btn.set_sensitive(current_zoom > min_zoom);
    zoom_in_btn.set_sensitive(current_zoom < max_zoom);
}

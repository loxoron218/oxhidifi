//! Application-wide keyboard shortcuts for the main window.
//!
//! Escape hides the sidebar when shown; `Ctrl+`/`Ctrl-` (including the numpad)
//! zoom the active view. Handlers return whether the key was handled so the
//! window's key controllers can stop propagation.

use libadwaita::{
    OverlaySplitView,
    gdk::{Key, ModifierType},
};

use crate::{
    app::runtime::AppState,
    ui::toggle_popover::{apply_zoom_in, apply_zoom_out, notify_zoom_change},
};

/// Hide the sidebar when Escape is pressed and the sidebar is shown.
///
/// Returns `true` when the key was handled (sidebar was visible and got
/// hidden), so the caller can stop propagation.
#[must_use]
pub fn handle_escape_key(split_view: &OverlaySplitView) -> bool {
    if split_view.shows_sidebar() {
        split_view.set_show_sidebar(false);
        true
    } else {
        false
    }
}

/// Zoom the active view in or out on `Ctrl+`/`Ctrl-` (including the numpad).
///
/// Only acts when the `Ctrl` modifier is pressed (ignoring unrelated
/// modifiers such as `ShiftLock`). The current view mode — grid or column —
/// determines whether the grid or list zoom level changes, matching the
/// popover zoom buttons. Zooming the active view fans out through
/// [`notify_zoom_change`] so the coalescer resizes (grid) or rebuilds
/// (column) the live view.
///
/// # Returns
///
/// `true` when the key was handled (a zoom occurred), `false` otherwise.
pub fn handle_zoom_key(state: &AppState, key: Key, modifiers: ModifierType) -> bool {
    if !modifiers.intersects(ModifierType::CONTROL_MASK) {
        return false;
    }
    let mode = state.storage.get_view_mode();
    if key == Key::plus || key == Key::KP_Add {
        apply_zoom_in(state, mode);
        notify_zoom_change(state);
        true
    } else if key == Key::minus || key == Key::KP_Subtract {
        apply_zoom_out(state, mode);
        notify_zoom_change(state);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            OverlaySplitView,
            gdk::{Key, ModifierType},
            gtk::{self, test as gtk_test},
        },
    };

    use crate::{
        app::{mocks::isolated_app_state, runtime::AppState},
        storage::view_mode::ViewMode::{Column, Grid},
        ui::{
            key_bindings::{handle_escape_key, handle_zoom_key},
            zoom::{GRID_ZOOM_MAX, GRID_ZOOM_MIN, LIST_ZOOM_MAX, LIST_ZOOM_MIN},
        },
    };

    #[gtk_test]
    fn escape_key_hides_shown_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            handle_escape_key(&split_view),
            "Escape on a shown sidebar must be handled"
        );
        ensure!(
            !split_view.shows_sidebar(),
            "Escape must hide the shown sidebar"
        );
        Ok(())
    }

    #[gtk_test]
    fn escape_key_ignores_hidden_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(false);
        ensure!(
            !handle_escape_key(&split_view),
            "Escape with a hidden sidebar must not be handled"
        );
        ensure!(!split_view.shows_sidebar());
        Ok(())
    }

    fn grid_state(zoom: u8) -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        state.storage.set_view_mode_memory(Grid);
        state.storage.set_grid_zoom_level_memory(zoom);
        Ok(state)
    }

    fn column_state(zoom: u8) -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        state.storage.set_view_mode_memory(Column);
        state.storage.set_list_zoom_level_memory(zoom);
        Ok(state)
    }

    fn zoom_grid_state(zoom: u8, key: Key) -> Result<Arc<AppState>> {
        let state = grid_state(zoom)?;
        ensure!(handle_zoom_key(&state, key, ModifierType::CONTROL_MASK));
        Ok(state)
    }

    fn zoom_column_state(zoom: u8, key: Key) -> Result<Arc<AppState>> {
        let state = column_state(zoom)?;
        ensure!(handle_zoom_key(&state, key, ModifierType::CONTROL_MASK));
        Ok(state)
    }

    #[test]
    fn ctrl_plus_zooms_in_grid() -> Result<()> {
        let state = zoom_grid_state(2, Key::plus)?;
        ensure!(
            state.storage.get_grid_zoom_level() == 3,
            "Ctrl+ must raise the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_minus_zooms_out_grid() -> Result<()> {
        let state = zoom_grid_state(2, Key::minus)?;
        ensure!(
            state.storage.get_grid_zoom_level() == 1,
            "Ctrl- must lower the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_numpad_add_zooms_in_grid() -> Result<()> {
        let state = zoom_grid_state(GRID_ZOOM_MIN, Key::KP_Add)?;
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MIN + 1,
            "Ctrl+KP_Add must raise the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_numpad_subtract_zooms_out_grid() -> Result<()> {
        let state = zoom_grid_state(GRID_ZOOM_MAX, Key::KP_Subtract)?;
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX - 1,
            "Ctrl+KP_Subtract must lower the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_without_control_modifier_ignored() -> Result<()> {
        let state = grid_state(2)?;
        ensure!(!handle_zoom_key(&state, Key::plus, ModifierType::empty()));
        ensure!(
            state.storage.get_grid_zoom_level() == 2,
            "zoom must not change without the Ctrl modifier"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_clamps_grid_at_max() -> Result<()> {
        let state = zoom_grid_state(GRID_ZOOM_MAX, Key::plus)?;
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX,
            "Ctrl+ must clamp at the maximum grid zoom"
        );
        Ok(())
    }

    #[test]
    fn ctrl_minus_clamps_grid_at_min() -> Result<()> {
        let state = zoom_grid_state(GRID_ZOOM_MIN, Key::minus)?;
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MIN,
            "Ctrl- must clamp at the minimum grid zoom"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_in_column_view_zooms_list() -> Result<()> {
        let state = zoom_column_state(1, Key::plus)?;
        ensure!(
            state.storage.get_list_zoom_level() == 2,
            "Ctrl+ in column view must raise the list zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_minus_in_column_view_clamps_list_at_min() -> Result<()> {
        let state = zoom_column_state(LIST_ZOOM_MIN, Key::minus)?;
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MIN,
            "Ctrl- in column view must clamp at the minimum list zoom"
        );
        Ok(())
    }

    #[test]
    fn unrelated_key_returns_false() -> Result<()> {
        let state = grid_state(2)?;
        ensure!(!handle_zoom_key(&state, Key::a, ModifierType::CONTROL_MASK));
        ensure!(
            state.storage.get_grid_zoom_level() == 2,
            "an unrelated Ctrl key must not zoom"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_in_column_view_clamps_list_at_max() -> Result<()> {
        let state = zoom_column_state(LIST_ZOOM_MAX, Key::plus)?;
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MAX,
            "Ctrl+ in column view must clamp at the maximum list zoom"
        );
        Ok(())
    }
}

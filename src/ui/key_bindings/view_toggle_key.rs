//! View-toggle shortcut for the main window.
//!
//! `Ctrl+G` toggles the library layout between grid and column views.
//! Handlers return whether the key was handled so the window's key
//! controllers can stop propagation.

use libadwaita::gdk::{Key, ModifierType};

use crate::app::runtime::AppState;

/// Toggle the library view mode on `Ctrl+G`.
///
/// Only acts on exact `Ctrl+G`/`Ctrl+g` (no `Shift`, matching the `Ctrl+B`
/// panel toggle). Returns `false` on the `Signal` tab since the view-switch
/// control is hidden there and mutating the background layout would surprise
/// the user. Detail pages are guarded by the caller (`add_key_controllers`
/// returns `Proceed` while a detail page is pushed), since detail covers use
/// fixed sizes.
///
/// Only updates in-memory settings and fans out through the `view_mode`
/// signal (header icon, zoom sensitivity, and gallery rebuilds follow via
/// existing subscriptions). The caller persists via debounced
/// [`SqliteStorage::save_settings`](crate::storage::database::SqliteStorage::save_settings),
/// keeping this handler synchronous and unit-testable without a Tokio or
/// `GLib` main context.
///
/// # Arguments
///
/// * `state` - Application state owning the view-mode signal and storage.
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers; must be exactly `Ctrl`.
///
/// # Returns
///
/// `true` when the view was toggled, `false` otherwise (Signal tab, wrong
/// modifiers, or unrelated key).
pub fn handle_view_toggle_key(state: &AppState, key: Key, modifiers: ModifierType) -> bool {
    if state.active_tab.borrow().is_signal() {
        return false;
    }
    if modifiers != ModifierType::CONTROL_MASK {
        return false;
    }
    if key != Key::g && key != Key::G {
        return false;
    }
    let mode = state.storage.get_view_mode().toggle();
    state.storage.set_view_mode_memory(mode);
    state.view_mode.send(mode);
    true
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gdk::{Key, ModifierType},
    };

    use crate::{
        app::mocks::isolated_app_state,
        storage::{
            active_tab::ActiveTab::Signal,
            view_mode::ViewMode::{Column, Grid},
        },
        ui::key_bindings::view_toggle_key::handle_view_toggle_key,
    };

    #[test]
    fn ctrl_g_toggles_grid_to_column() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Grid);
        ensure!(
            handle_view_toggle_key(&state, Key::g, ModifierType::CONTROL_MASK),
            "Ctrl+G on grid must be handled"
        );
        ensure!(
            state.storage.get_view_mode() == Column,
            "Ctrl+G must switch grid to column"
        );
        Ok(())
    }

    #[test]
    fn ctrl_shift_g_toggles_column_to_grid() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Column);
        ensure!(
            handle_view_toggle_key(&state, Key::G, ModifierType::CONTROL_MASK),
            "Ctrl+G (shifted keyval) on column must be handled"
        );
        ensure!(
            state.storage.get_view_mode() == Grid,
            "Ctrl+G must switch column to grid"
        );
        Ok(())
    }

    #[test]
    fn toggle_notifies_view_mode_subscribers() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Grid);
        let rx = state.view_mode.subscribe();
        ensure!(handle_view_toggle_key(
            &state,
            Key::g,
            ModifierType::CONTROL_MASK
        ));
        ensure!(
            matches!(rx.try_recv(), Ok(Column)),
            "toggle must notify subscribers with the new mode"
        );
        Ok(())
    }

    #[test]
    fn toggle_without_ctrl_is_ignored() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Grid);
        ensure!(
            !handle_view_toggle_key(&state, Key::g, ModifierType::empty()),
            "G without Ctrl must not toggle"
        );
        ensure!(
            state.storage.get_view_mode() == Grid,
            "view mode must not change without Ctrl"
        );
        Ok(())
    }

    #[test]
    fn ctrl_shift_modifier_is_ignored() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Grid);
        let shifted = ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK;
        ensure!(
            !handle_view_toggle_key(&state, Key::g, shifted),
            "Ctrl+Shift+G must fall through for future use"
        );
        ensure!(
            state.storage.get_view_mode() == Grid,
            "view mode must not change on Ctrl+Shift+G"
        );
        Ok(())
    }

    #[test]
    fn unrelated_ctrl_key_is_ignored() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Grid);
        ensure!(
            !handle_view_toggle_key(&state, Key::a, ModifierType::CONTROL_MASK),
            "an unrelated Ctrl key must not toggle"
        );
        ensure!(
            state.storage.get_view_mode() == Grid,
            "view mode must not change on unrelated keys"
        );
        Ok(())
    }

    #[test]
    fn signal_tab_is_ignored() -> Result<()> {
        let state = isolated_app_state()?;
        state.storage.set_view_mode_memory(Grid);
        state.active_tab.send(Signal);
        ensure!(
            !handle_view_toggle_key(&state, Key::g, ModifierType::CONTROL_MASK),
            "Ctrl+G on the Signal tab must be ignored"
        );
        ensure!(
            state.storage.get_view_mode() == Grid,
            "view mode must not change on the Signal tab"
        );
        Ok(())
    }
}

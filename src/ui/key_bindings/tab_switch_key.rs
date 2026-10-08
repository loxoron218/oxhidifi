//! Tab cycling shortcuts for the main window.
//!
//! `Ctrl+Tab` advances to the next library tab (albums, artists, signal,
//! wrapping around) and `Ctrl+Shift+Tab` goes back. Handlers return whether
//! the key was handled so the window's key controllers can stop propagation.

use libadwaita::gdk::{Key, ModifierType};

use crate::{
    app::runtime::AppState, storage::active_tab::ActiveTab, ui::navigation::persist_active_tab,
};

/// Resolve the destination tab for a `Ctrl+Tab` key press.
///
/// Only acts when the `Ctrl` modifier is pressed (ignoring unrelated
/// modifiers, matching the zoom handler). Plain `Ctrl+Tab` advances via
/// [`ActiveTab::next`]; `Ctrl+Shift+Tab` — or `Ctrl+ISO_Left_Tab`, which some
/// layouts emit for shifted tab — goes back via [`ActiveTab::prev`].
///
/// # Arguments
///
/// * `current` - Currently active tab.
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers.
///
/// # Returns
///
/// The destination tab, or `None` for unrelated keys or missing `Ctrl`.
#[must_use]
pub fn next_tab_for_key(
    current: ActiveTab,
    key: Key,
    modifiers: ModifierType,
) -> Option<ActiveTab> {
    if !modifiers.intersects(ModifierType::CONTROL_MASK) {
        return None;
    }
    if key == Key::Tab {
        if modifiers.intersects(ModifierType::SHIFT_MASK) {
            Some(current.prev())
        } else {
            Some(current.next())
        }
    } else if key == Key::ISO_Left_Tab {
        Some(current.prev())
    } else {
        None
    }
}

/// Cycle the library tab on `Ctrl+Tab` / `Ctrl+Shift+Tab`.
///
/// Persists through [`persist_active_tab`] so storage and the tab signal
/// stay in sync exactly like pointer-driven switches. Intentionally works
/// while a detail page is pushed (unlike zoom): the stack change behind the
/// detail fires tab tracking, which sends `Back` and lands on the library
/// with the new tab visible. Also intentionally works from text entries:
/// `Ctrl+Tab` has no editing function to preserve.
///
/// # Arguments
///
/// * `state` - Application state owning the tab signal and storage.
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers.
///
/// # Returns
///
/// `true` when the tab was switched, `false` otherwise (wrong modifiers or
/// unrelated key).
pub fn handle_tab_switch_key(state: &AppState, key: Key, modifiers: ModifierType) -> bool {
    let Some(next) = next_tab_for_key(state.active_tab.borrow(), key, modifiers) else {
        return false;
    };
    persist_active_tab(&state.storage, &state.active_tab, next.stack_name());
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
        storage::active_tab::ActiveTab::{Albums, Artists, Signal},
        ui::key_bindings::tab_switch_key::{handle_tab_switch_key, next_tab_for_key},
    };

    #[test]
    fn ctrl_tab_advances_forward() -> Result<()> {
        ensure!(next_tab_for_key(Albums, Key::Tab, ModifierType::CONTROL_MASK) == Some(Artists));
        ensure!(next_tab_for_key(Artists, Key::Tab, ModifierType::CONTROL_MASK) == Some(Signal));
        ensure!(next_tab_for_key(Signal, Key::Tab, ModifierType::CONTROL_MASK) == Some(Albums));
        Ok(())
    }

    #[test]
    fn ctrl_shift_tab_goes_back() -> Result<()> {
        let backward = ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK;
        ensure!(next_tab_for_key(Albums, Key::Tab, backward) == Some(Signal));
        ensure!(next_tab_for_key(Artists, Key::Tab, backward) == Some(Albums));
        ensure!(next_tab_for_key(Signal, Key::Tab, backward) == Some(Artists));
        Ok(())
    }

    #[test]
    fn ctrl_iso_left_tab_goes_back() -> Result<()> {
        ensure!(
            next_tab_for_key(Artists, Key::ISO_Left_Tab, ModifierType::CONTROL_MASK)
                == Some(Albums)
        );
        Ok(())
    }

    #[test]
    fn tab_without_ctrl_is_ignored() -> Result<()> {
        ensure!(next_tab_for_key(Albums, Key::Tab, ModifierType::empty()).is_none());
        ensure!(
            next_tab_for_key(Albums, Key::Tab, ModifierType::SHIFT_MASK).is_none(),
            "Shift alone must not switch tabs"
        );
        Ok(())
    }

    #[test]
    fn unrelated_key_is_ignored() -> Result<()> {
        ensure!(
            next_tab_for_key(Albums, Key::a, ModifierType::CONTROL_MASK).is_none(),
            "an unrelated Ctrl key must not switch tabs"
        );
        Ok(())
    }

    #[test]
    fn handle_switches_active_tab_and_notifies() -> Result<()> {
        let state = isolated_app_state()?;
        let rx = state.active_tab.subscribe();
        ensure!(handle_tab_switch_key(
            &state,
            Key::Tab,
            ModifierType::CONTROL_MASK
        ));
        ensure!(
            state.active_tab.borrow() == Artists,
            "Ctrl+Tab must advance from the initial albums tab"
        );
        ensure!(matches!(rx.try_recv(), Ok(Artists)));
        Ok(())
    }

    #[test]
    fn handle_returns_false_without_ctrl() -> Result<()> {
        let state = isolated_app_state()?;
        ensure!(!handle_tab_switch_key(
            &state,
            Key::Tab,
            ModifierType::empty()
        ));
        ensure!(
            state.active_tab.borrow() == Albums,
            "tab must not change without the Ctrl modifier"
        );
        Ok(())
    }
}

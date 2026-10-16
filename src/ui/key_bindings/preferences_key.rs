//! Preferences-dialog shortcut for the main window.
//!
//! `Ctrl+,` opens the preferences dialog. Keyval and modifier matching live
//! here so the window's key controllers stay thin; presenting the dialog
//! itself stays in the sibling [`preferences`](crate::ui::preferences)
//! module.

use libadwaita::gdk::{Key, ModifierType};

/// Whether the pressed key opens the preferences dialog (`Ctrl+,`).
///
/// Only acts when the `Ctrl` modifier is pressed (ignoring unrelated extra
/// modifiers, matching the zoom handler). Intentionally works from text
/// entries: `Ctrl+,` has no text editing function to preserve, matching the
/// `Ctrl+Tab` and `Ctrl+?` precedents.
///
/// # Arguments
///
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers.
///
/// # Returns
///
/// `true` when the dialog opener was pressed, `false` otherwise.
#[must_use]
pub fn is_preferences_key(key: Key, modifiers: ModifierType) -> bool {
    if !modifiers.intersects(ModifierType::CONTROL_MASK) {
        return false;
    }
    key == Key::comma
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gdk::{Key, ModifierType},
    };

    use crate::ui::key_bindings::preferences_key::is_preferences_key;

    #[test]
    fn ctrl_comma_opens_preferences() -> Result<()> {
        ensure!(
            is_preferences_key(Key::comma, ModifierType::CONTROL_MASK),
            "Ctrl+, must open the preferences dialog"
        );
        ensure!(
            is_preferences_key(
                Key::comma,
                ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK
            ),
            "Ctrl+Shift+, must open the dialog (Shift folded into modifiers)"
        );
        Ok(())
    }

    #[test]
    fn preferences_key_ignores_unrelated_keys() -> Result<()> {
        ensure!(
            !is_preferences_key(Key::comma, ModifierType::empty()),
            ", without Ctrl must not open the dialog"
        );
        ensure!(
            !is_preferences_key(Key::period, ModifierType::CONTROL_MASK),
            "an unrelated Ctrl key must not open the dialog"
        );
        ensure!(
            !is_preferences_key(Key::a, ModifierType::CONTROL_MASK),
            "an unrelated Ctrl key must not open the dialog"
        );
        Ok(())
    }
}

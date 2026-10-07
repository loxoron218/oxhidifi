//! Window geometry and side panel visibility for user settings.

use serde::{Deserialize, Serialize};

/// Window geometry and side panel visibility.
///
/// Flattened into [`UserSettings`](crate::storage::settings::UserSettings) so
/// `settings.json` keeps its flat shape (`window_width`, `window_height`,
/// `window_maximized`, `sidebar_visible`). Grouping the window bools here
/// keeps each settings struct under the `struct_excessive_bools` limit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    /// Stored window width.
    #[serde(rename = "window_width")]
    pub width: i32,
    /// Stored window height.
    #[serde(rename = "window_height")]
    pub height: i32,
    /// Whether window is maximized.
    #[serde(rename = "window_maximized")]
    pub maximized: bool,
    /// Whether the side player panel (sidebar) is visible.
    pub sidebar_visible: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 800,
            maximized: false,
            sidebar_visible: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        serde_json::{from_str, to_string_pretty},
    };

    use crate::storage::window_state::WindowState;

    #[test]
    fn window_state_defaults_to_unmaximized_hidden_panel() -> Result<()> {
        let state = WindowState::default();
        ensure!(
            state.width == 1200 && state.height == 800,
            "default geometry must be 1200x800"
        );
        ensure!(
            !state.maximized && !state.sidebar_visible,
            "default window must be unmaximized with a hidden panel"
        );
        Ok(())
    }

    #[test]
    fn window_state_serializes_with_legacy_flat_keys() -> Result<()> {
        let original = WindowState {
            width: 960,
            height: 640,
            maximized: true,
            sidebar_visible: true,
        };
        let json = to_string_pretty(&original)?;
        ensure!(
            json.contains("\"window_width\"") && json.contains("\"sidebar_visible\""),
            "window state must keep the legacy flat key names"
        );
        ensure!(
            !json.contains("\"window\":"),
            "window state must not nest under a window object"
        );
        let restored: WindowState = from_str(&json)?;
        ensure!(
            restored.width == 960
                && restored.height == 640
                && restored.maximized
                && restored.sidebar_visible,
            "window state must round-trip through JSON"
        );
        Ok(())
    }
}

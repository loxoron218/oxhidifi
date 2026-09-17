//! Library view mode preference.

use serde::{Deserialize, Serialize};

/// User-facing view mode preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Grid layout.
    Grid,
    /// Column/list layout.
    Column,
}

impl ViewMode {
    /// Get the icon name for this view mode.
    #[must_use]
    pub const fn icon_name(self) -> &'static str {
        match self {
            Self::Grid => "view-grid-symbolic",
            Self::Column => "view-list-symbolic",
        }
    }

    /// Get the tooltip text for this view mode.
    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Grid => "Switch to column view",
            Self::Column => "Switch to grid view",
        }
    }

    /// Get the other view mode.
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Grid => Self::Column,
            Self::Column => Self::Grid,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        playback::devices::OutputMode::Resampled,
        storage::{
            settings::UserSettings,
            view_mode::ViewMode::{Column, Grid},
        },
    };

    #[test]
    fn defaults_to_grid() {
        let settings = UserSettings::default();
        assert_eq!(settings.view_mode, Grid);
        assert_eq!(settings.output_mode, Resampled);
    }

    #[test]
    fn view_mode_icons_and_tooltips() {
        assert_eq!(Grid.icon_name(), "view-grid-symbolic");
        assert_eq!(Column.icon_name(), "view-list-symbolic");
        assert_eq!(Grid.tooltip(), "Switch to column view");
        assert_eq!(Column.tooltip(), "Switch to grid view");
    }
}

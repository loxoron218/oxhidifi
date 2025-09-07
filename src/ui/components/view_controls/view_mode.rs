use std::default::Default;

use serde::{Deserialize, Serialize};

/// Represents the different view modes available in the application
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Grid view layout for displaying content in a grid format
    GridView,
    /// List view layout for displaying content in a list format
    ListView,
}

impl Default for ViewMode {
    /// Returns the default view mode, which is GridView.
    ///
    /// # Returns
    ///
    /// The default `ViewMode` variant, `GridView`.
    fn default() -> Self {
        ViewMode::GridView
    }
}

impl ViewMode {
    /// Returns the icon name associated with the view mode
    ///
    /// # Returns
    ///
    /// A static string slice representing the icon name for the view mode
    pub fn icon_name(&self) -> &'static str {
        match self {
            ViewMode::GridView => "view-grid-symbolic",
            ViewMode::ListView => "view-list-symbolic",
        }
    }

    /// Returns the tooltip text associated with the view mode
    ///
    /// # Returns
    ///
    /// A static string slice representing the tooltip text for the view mode
    pub fn tooltip_text(&self) -> &'static str {
        match self {
            ViewMode::GridView => "Grid View",
            ViewMode::ListView => "List View",
        }
    }
}

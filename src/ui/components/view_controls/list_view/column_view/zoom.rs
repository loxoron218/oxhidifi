use serde::{Deserialize, Serialize};

/// Represents the different zoom levels available for the ColumnView.
///
/// The zoom levels control the size of album covers and column widths in the list view.
/// Each variant corresponds to a specific cover size configuration optimized for list view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ColumnViewZoomLevel {
    /// Extra compact zoom level with minimal cover size.
    ExtraCompact,
    /// Compact zoom level with reduced cover size.
    Compact,
    /// Normal zoom level, the default setting with balanced cover size.
    Normal,
    /// Expanded zoom level with increased cover size.
    Expanded,
    /// Extra expanded zoom level with maximum cover size.
    ExtraExpanded,
}

impl ColumnViewZoomLevel {
    /// Returns the next zoom level in the sequence.
    ///
    /// Cycles through zoom levels from `ExtraCompact` to `ExtraExpanded`.
    /// When at the maximum zoom level (`ExtraExpanded`), it stays at that level.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::zoom::ColumnViewZoomLevel;
    ///
    /// let current = ColumnViewZoomLevel::Compact;
    /// let next = current.next();
    /// assert_eq!(next, ColumnViewZoomLevel::Normal);
    /// ```
    pub fn next(&self) -> ColumnViewZoomLevel {
        match self {
            ColumnViewZoomLevel::ExtraCompact => ColumnViewZoomLevel::Compact,
            ColumnViewZoomLevel::Compact => ColumnViewZoomLevel::Normal,
            ColumnViewZoomLevel::Normal => ColumnViewZoomLevel::Expanded,
            ColumnViewZoomLevel::Expanded => ColumnViewZoomLevel::ExtraExpanded,
            ColumnViewZoomLevel::ExtraExpanded => ColumnViewZoomLevel::ExtraExpanded,
        }
    }

    /// Returns the previous zoom level in the sequence.
    ///
    /// Cycles through zoom levels from `ExtraExpanded` to `ExtraCompact`.
    /// When at the minimum zoom level (`ExtraCompact`), it stays at that level.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::zoom::ColumnViewZoomLevel;
    ///
    /// let current = ColumnViewZoomLevel::Expanded;
    /// let previous = current.previous();
    /// assert_eq!(previous, ColumnViewZoomLevel::Normal);
    /// ```
    pub fn previous(&self) -> ColumnViewZoomLevel {
        match self {
            ColumnViewZoomLevel::ExtraCompact => ColumnViewZoomLevel::ExtraCompact,
            ColumnViewZoomLevel::Compact => ColumnViewZoomLevel::ExtraCompact,
            ColumnViewZoomLevel::Normal => ColumnViewZoomLevel::Compact,
            ColumnViewZoomLevel::Expanded => ColumnViewZoomLevel::Normal,
            ColumnViewZoomLevel::ExtraExpanded => ColumnViewZoomLevel::Expanded,
        }
    }

    /// Returns the cover size in pixels for this zoom level.
    ///
    /// The cover size determines the dimensions of album artwork displayed
    /// in the ColumnView. The `Normal` level matches the current implementation (48px).
    ///
    /// # Returns
    ///
    /// The size in pixels as an `i32`.
    pub fn cover_size(&self) -> i32 {
        match self {
            ColumnViewZoomLevel::ExtraCompact => 32,
            ColumnViewZoomLevel::Compact => 40,
            ColumnViewZoomLevel::Normal => 48,
            ColumnViewZoomLevel::Expanded => 64,
            ColumnViewZoomLevel::ExtraExpanded => 80,
        }
    }

    /// Returns recommended column widths for this zoom level.
    ///
    /// These values are optimized for each zoom level to ensure proper spacing
    /// and readability in the ColumnView.
    ///
    /// # Returns
    ///
    /// A tuple containing (cover_column_width, dr_column_width) in pixels as `i32`.
    pub fn column_widths(&self) -> (i32, i32) {
        match self {
            ColumnViewZoomLevel::ExtraCompact => (40, 40),
            ColumnViewZoomLevel::Compact => (50, 45),
            ColumnViewZoomLevel::Normal => (60, 50),
            ColumnViewZoomLevel::Expanded => (75, 55),
            ColumnViewZoomLevel::ExtraExpanded => (90, 60),
        }
    }
}

impl Default for ColumnViewZoomLevel {
    /// Returns the default zoom level for the ColumnView.
    ///
    /// The default zoom level is `Normal`, providing a balanced view
    /// of album covers and associated information.
    ///
    /// # Returns
    ///
    /// The default `ColumnViewZoomLevel` variant, `Normal`.
    fn default() -> Self {
        ColumnViewZoomLevel::Normal
    }
}

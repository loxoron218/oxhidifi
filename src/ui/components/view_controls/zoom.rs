use serde::{Deserialize, Serialize};

/// Represents the different zoom levels available in the application.
///
/// The zoom levels control the size of album covers, tiles, and the amount of
/// text information displayed in the grid views. Each variant corresponds to
/// a specific size configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ZoomLevel {
    /// Extra small zoom level with minimal cover size and text.
    ExtraSmall,
    /// Small zoom level with reduced cover size and text.
    Small,
    /// Medium zoom level, the default setting with balanced cover size and text.
    Medium,
    /// Large zoom level with increased cover size and text.
    Large,
    /// Extra large zoom level with maximum cover size and text.
    ExtraLarge,
}

impl ZoomLevel {
    /// Returns the next zoom level in the sequence.
    ///
    /// Cycles through zoom levels from `ExtraSmall` to `ExtraLarge`.
    /// When at the maximum zoom level (`ExtraLarge`), it stays at that level.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::zoom::ZoomLevel;
    ///
    /// let current = ZoomLevel::Small;
    /// let next = current.next();
    /// assert_eq!(next, ZoomLevel::Medium);
    /// ```
    pub fn next(&self) -> ZoomLevel {
        match self {
            ZoomLevel::ExtraSmall => ZoomLevel::Small,
            ZoomLevel::Small => ZoomLevel::Medium,
            ZoomLevel::Medium => ZoomLevel::Large,
            ZoomLevel::Large => ZoomLevel::ExtraLarge,
            ZoomLevel::ExtraLarge => ZoomLevel::ExtraLarge,
        }
    }

    /// Returns the previous zoom level in the sequence.
    ///
    /// Cycles through zoom levels from `ExtraLarge` to `ExtraSmall`.
    /// When at the minimum zoom level (`ExtraSmall`), it stays at that level.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::zoom::ZoomLevel;
    ///
    /// let current = ZoomLevel::Large;
    /// let previous = current.previous();
    /// assert_eq!(previous, ZoomLevel::Medium);
    /// ```
    pub fn previous(&self) -> ZoomLevel {
        match self {
            ZoomLevel::ExtraSmall => ZoomLevel::ExtraSmall,
            ZoomLevel::Small => ZoomLevel::ExtraSmall,
            ZoomLevel::Medium => ZoomLevel::Small,
            ZoomLevel::Large => ZoomLevel::Medium,
            ZoomLevel::ExtraLarge => ZoomLevel::Large,
        }
    }

    /// Returns the cover size in pixels for this zoom level.
    ///
    /// The cover size determines the dimensions of album artwork displayed
    /// in the grid views. The `Medium` level is dynamically adjusted based
    /// on screen size in the application.
    ///
    /// # Returns
    ///
    /// The size in pixels as an `i32`.
    pub fn cover_size(&self) -> i32 {
        match self {
            ZoomLevel::ExtraSmall => 64,
            ZoomLevel::Small => 96,
            ZoomLevel::Medium => 128,
            ZoomLevel::Large => 256,
            ZoomLevel::ExtraLarge => 384,
        }
    }

    /// Returns the tile size in pixels for this zoom level.
    ///
    /// The tile size determines the overall dimensions of grid items,
    /// including the cover art and associated text information.
    /// Currently, tile size equals cover size but may be adjusted separately in the future.
    ///
    /// # Returns
    ///
    /// The size in pixels as an `i32`.
    pub fn tile_size(&self) -> i32 {
        self.cover_size()
    }
}

impl Default for ZoomLevel {
    /// Returns the default zoom level for the application.
    ///
    /// The default zoom level is `Medium`, providing a balanced view
    /// of album covers and associated information.
    ///
    /// # Returns
    ///
    /// The default `ZoomLevel` variant, `Medium`.
    fn default() -> Self {
        ZoomLevel::Medium
    }
}

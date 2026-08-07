//! Zoom level to cover size mapping.
//!
//! Pure-function module mapping zoom levels to pixel dimensions for
//! grid and list view cover art. No state or channels.

/// Minimum grid zoom level.
pub const GRID_ZOOM_MIN: u8 = 0;

/// Maximum grid zoom level.
pub const GRID_ZOOM_MAX: u8 = 4;

/// Minimum list zoom level.
pub const LIST_ZOOM_MIN: u8 = 0;

/// Maximum list zoom level.
pub const LIST_ZOOM_MAX: u8 = 2;

/// Default grid zoom level (level 2 → 180 px covers).
pub const DEFAULT_GRID_ZOOM: u8 = 2;

/// Default list zoom level (level 1 → 48 px covers).
pub const DEFAULT_LIST_ZOOM: u8 = 1;

/// Map a grid zoom level to cover art size in pixels.
///
/// Level 0 → 120, 1 → 150, 2 → 180 (default), 3 → 210, 4 → 240.
#[must_use]
pub const fn grid_cover_size(level: u8) -> i32 {
    120 + (level as i32) * 30
}

/// Map a list zoom level to cover art size in pixels.
///
/// Level 0 → 32, 1 → 48 (default), 2 → 64.
#[must_use]
pub const fn list_cover_size(level: u8) -> i32 {
    32 + (level as i32) * 16
}

#[cfg(test)]
mod tests {
    use crate::zoom::{
        DEFAULT_GRID_ZOOM, DEFAULT_LIST_ZOOM, GRID_ZOOM_MAX, GRID_ZOOM_MIN, LIST_ZOOM_MAX,
        LIST_ZOOM_MIN, grid_cover_size, list_cover_size,
    };

    #[test]
    fn grid_cover_size_maps_every_level() {
        let expected = [(0, 120), (1, 150), (2, 180), (3, 210), (4, 240)];
        for (level, size) in expected {
            assert_eq!(
                grid_cover_size(level),
                size,
                "grid level {level} must map to {size} px"
            );
        }
    }

    #[test]
    fn list_cover_size_maps_every_level() {
        let expected = [(0, 32), (1, 48), (2, 64)];
        for (level, size) in expected {
            assert_eq!(
                list_cover_size(level),
                size,
                "list level {level} must map to {size} px"
            );
        }
    }

    #[test]
    fn grid_zoom_bounds_and_default() {
        assert_eq!(GRID_ZOOM_MIN, 0);
        assert_eq!(GRID_ZOOM_MAX, 4);
        assert!(
            (GRID_ZOOM_MIN..=GRID_ZOOM_MAX).contains(&DEFAULT_GRID_ZOOM),
            "default grid zoom must lie within the grid zoom range"
        );
        assert_eq!(
            grid_cover_size(DEFAULT_GRID_ZOOM),
            180,
            "the documented default grid cover size is 180 px"
        );
    }

    #[test]
    fn list_zoom_bounds_and_default() {
        assert_eq!(LIST_ZOOM_MIN, 0);
        assert_eq!(LIST_ZOOM_MAX, 2);
        assert!(
            (LIST_ZOOM_MIN..=LIST_ZOOM_MAX).contains(&DEFAULT_LIST_ZOOM),
            "default list zoom must lie within the list zoom range"
        );
        assert_eq!(
            list_cover_size(DEFAULT_LIST_ZOOM),
            48,
            "the documented default list cover size is 48 px"
        );
    }
}

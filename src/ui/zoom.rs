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

/// Maximum grid zoom level honored while the window is narrow.
///
/// Below the 700 px narrow breakpoint the library keeps two columns across at
/// the 360 px minimum window width. Level 0 (120 px, row `2*120+8=256 px`
/// plus card chrome) is the largest zoom step whose two-column row — including
/// the card CSS padding the plain cover math misses — fits there; larger
/// requested levels snap down to it without changing the stored user
/// preference.
pub const NARROW_GRID_ZOOM_MAX: u8 = 0;

/// Default list zoom level (level 1 → 48 px covers).
pub const DEFAULT_LIST_ZOOM: u8 = 1;

/// Map a grid zoom level to cover art size in pixels.
///
/// Level 0 → 120, 1 → 150, 2 → 180 (default), 3 → 210, 4 → 240.
#[must_use]
pub fn grid_cover_size(level: u8) -> i32 {
    120_i32.saturating_add(i32::from(level).saturating_mul(30))
}

/// Map a list zoom level to cover art size in pixels.
///
/// Level 0 → 32, 1 → 48 (default), 2 → 64.
#[must_use]
pub fn list_cover_size(level: u8) -> i32 {
    32_i32.saturating_add(i32::from(level).saturating_mul(16))
}

/// Resolve the grid zoom level to render for the current window width.
///
/// Returns `requested` on wide windows; while `narrow` (below the 700 px
/// breakpoint) clamps to [`NARROW_GRID_ZOOM_MAX`] so two columns keep fitting
/// the 360 px minimum window. The stored preference is never mutated — widening
/// the window restores the full requested size.
///
/// # Arguments
///
/// * `requested` - User's stored grid zoom level (0–[`GRID_ZOOM_MAX`]).
/// * `narrow` - Whether the window is below the narrow breakpoint.
///
/// # Returns
///
/// The zoom level whose cover size should be rendered.
#[must_use]
pub const fn effective_grid_zoom(requested: u8, narrow: bool) -> u8 {
    if narrow && requested > NARROW_GRID_ZOOM_MAX {
        NARROW_GRID_ZOOM_MAX
    } else {
        requested
    }
}

/// Resolve the grid cover size to render for the current window width.
///
/// Convenience wrapper over [`effective_grid_zoom`] + [`grid_cover_size`].
///
/// # Arguments
///
/// * `requested` - User's stored grid zoom level (0–[`GRID_ZOOM_MAX`]).
/// * `narrow` - Whether the window is below the narrow breakpoint.
///
/// # Returns
///
/// Cover art size in pixels to render.
#[must_use]
pub fn effective_grid_cover_size(requested: u8, narrow: bool) -> i32 {
    grid_cover_size(effective_grid_zoom(requested, narrow))
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::settings::{DEFAULT_GRID_ZOOM, DEFAULT_LIST_ZOOM},
        ui::zoom::{
            DEFAULT_GRID_ZOOM as UI_DEFAULT_GRID_ZOOM, DEFAULT_LIST_ZOOM as UI_DEFAULT_LIST_ZOOM,
            GRID_ZOOM_MAX, GRID_ZOOM_MIN, LIST_ZOOM_MAX, LIST_ZOOM_MIN, NARROW_GRID_ZOOM_MAX,
            effective_grid_cover_size, effective_grid_zoom, grid_cover_size, list_cover_size,
        },
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

    #[test]
    fn ui_default_zooms_match_documented_sizes() {
        assert_eq!(
            UI_DEFAULT_GRID_ZOOM, 2,
            "the ui default grid zoom must be level 2"
        );
        assert_eq!(
            UI_DEFAULT_LIST_ZOOM, 1,
            "the ui default list zoom must be level 1"
        );
        assert_eq!(
            grid_cover_size(UI_DEFAULT_GRID_ZOOM),
            180,
            "the ui default grid zoom must map to 180 px"
        );
        assert_eq!(
            list_cover_size(UI_DEFAULT_LIST_ZOOM),
            48,
            "the ui default list zoom must map to 48 px"
        );
    }

    #[test]
    fn effective_grid_zoom_passes_through_on_wide() {
        for level in GRID_ZOOM_MIN..=GRID_ZOOM_MAX {
            assert_eq!(
                effective_grid_zoom(level, false),
                level,
                "wide windows must render the requested level {level}"
            );
        }
    }

    #[test]
    fn effective_grid_zoom_clamps_to_narrow_max() {
        assert_eq!(NARROW_GRID_ZOOM_MAX, 0);
        let expected = [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)];
        for (requested, effective) in expected {
            assert_eq!(
                effective_grid_zoom(requested, true),
                effective,
                "narrow windows must snap requested level {requested} to {effective}"
            );
        }
    }

    #[test]
    fn effective_grid_cover_size_snaps_without_remapping() {
        assert_eq!(
            effective_grid_cover_size(2, false),
            180,
            "wide windows must render the default 180 px covers"
        );
        assert_eq!(
            effective_grid_cover_size(4, false),
            240,
            "wide windows must render the maximum 240 px covers"
        );
        assert_eq!(
            effective_grid_cover_size(2, true),
            120,
            "narrow windows must shrink default covers to 120 px"
        );
        assert_eq!(
            effective_grid_cover_size(4, true),
            120,
            "narrow windows must shrink maximum covers to 120 px"
        );
    }
}

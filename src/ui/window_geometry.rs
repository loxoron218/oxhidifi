//! Window geometry snapshot, clamp, and minimum size.
//!
//! The compositor (Mutter on Wayland) never updates `default_width()` while a
//! window is tiled/snapped, so geometry persistence must snapshot the actual
//! allocation instead. Breakpoints additionally remove the automatic window
//! minimum, which [`MIN_WINDOW_WIDTH`]/[`MIN_WINDOW_HEIGHT`] restore.

use libadwaita::{
    ApplicationWindow,
    prelude::{GtkWindowExt, WidgetExt},
};

/// Minimum window width in pixels.
///
/// Required because `AdwApplicationWindow` with breakpoints has no automatic
/// minimum size; without an explicit request the window can briefly measure
/// children (e.g. `ViewSwitcherBar`'s `GtkRevealer`, library `ScrolledWindow`)
/// at ~30 px, producing `Trying to measure ... for width of 30` warnings.
pub const MIN_WINDOW_WIDTH: i32 = 360;

/// Minimum window height in pixels.
///
/// Matches the `AdwWindow` default documented in libadwaita.
pub const MIN_WINDOW_HEIGHT: i32 = 200;

/// Fallback width used when stored settings are missing or corrupt.
const FALLBACK_WIDTH: i32 = 1200;

/// Fallback height used when stored settings are missing or corrupt.
const FALLBACK_HEIGHT: i32 = 800;

/// Clamp stored geometry to sane values.
///
/// Guards against corrupt `settings.json` entries (`<= 0`) and against sizes
/// below the window minimum that would trigger measure warnings.
#[must_use]
pub const fn clamp_geometry(width: i32, height: i32) -> (i32, i32) {
    let width = if width < MIN_WINDOW_WIDTH {
        FALLBACK_WIDTH
    } else {
        width
    };
    let height = if height < MIN_WINDOW_HEIGHT {
        FALLBACK_HEIGHT
    } else {
        height
    };
    (width, height)
}

/// Snapshot the geometry to persist.
///
/// Returns `(width, height, maximized_or_fullscreen)`. When the window is
/// maximized or fullscreen the compositor owns the current allocation, so the
/// last stored floating size is kept and only the state flag is reported. In
/// every other state — floating **or compositor-tiled/snapped** — the actual
/// allocated size is returned: `default_width()` still holds the old floating
/// size while tiled (Mutter never updates it), so saving it would reopen a
/// half-width tiled window at full width. Saving the allocation reopens the
/// tiled size as a floating window, which is the closest Wayland allows
/// (position/snapping cannot be restored by apps).
#[must_use]
pub fn snapshot_geometry(window: &ApplicationWindow) -> (i32, i32, bool) {
    if window.is_maximized() || window.is_fullscreen() {
        let (width, height) = clamp_geometry(window.default_width(), window.default_height());
        return (width, height, true);
    }
    let allocated = (window.width(), window.height());
    let fallback = (window.default_width(), window.default_height());
    let width = if allocated.0 > 0 {
        allocated.0
    } else {
        fallback.0
    };
    let height = if allocated.1 > 0 {
        allocated.1
    } else {
        fallback.1
    };
    let (width, height) = clamp_geometry(width, height);
    (width, height, false)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::window_geometry::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, clamp_geometry};

    #[test]
    fn clamp_geometry_keeps_sane_values() -> Result<()> {
        let (width, height) = clamp_geometry(960, 640);
        ensure!(
            width == 960 && height == 640,
            "a tiled half-width size must pass through unchanged"
        );
        Ok(())
    }

    #[test]
    fn clamp_geometry_rejects_corrupt_values() -> Result<()> {
        let (width, height) = clamp_geometry(0, -50);
        ensure!(
            width >= MIN_WINDOW_WIDTH && height >= MIN_WINDOW_HEIGHT,
            "corrupt settings must fall back to a usable size"
        );
        Ok(())
    }

    #[test]
    fn clamp_geometry_rejects_below_minimum() -> Result<()> {
        let (width, height) = clamp_geometry(30, 30);
        ensure!(
            width >= MIN_WINDOW_WIDTH && height >= MIN_WINDOW_HEIGHT,
            "sub-minimum sizes that trigger measure warnings must fall back"
        );
        Ok(())
    }
}

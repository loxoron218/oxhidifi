//! Minimum-window-width assertion shared by detail-page tests.
//!
//! Measures a widget's horizontal minimum against
//! [`MIN_WINDOW_WIDTH`](crate::ui::window_geometry::MIN_WINDOW_WIDTH) so
//! detail pages can never force horizontal scrolling on the smallest window.

use {
    anyhow::{Result, ensure},
    libadwaita::{gtk::Orientation::Horizontal, prelude::WidgetExt},
};

use crate::ui::window_geometry::MIN_WINDOW_WIDTH;

/// Assert a widget's minimum width fits the minimum window width.
///
/// # Arguments
///
/// * `widget` - Widget to measure horizontally.
/// * `name` - Widget description for the failure message.
///
/// # Returns
///
/// * `Result<()>` - `Ok` when the widget fits the minimum window.
///
/// # Errors
///
/// Returns an error when the widget's minimum width exceeds the minimum
/// window width.
pub fn assert_fits_minimum_window(widget: &impl WidgetExt, name: &str) -> Result<()> {
    let (minimum, _, _, _) = widget.measure(Horizontal, -1);
    ensure!(
        minimum <= MIN_WINDOW_WIDTH,
        "{name} {minimum} px must fit the {MIN_WINDOW_WIDTH} px minimum window"
    );
    Ok(())
}

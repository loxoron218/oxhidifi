use std::{cell::Cell, rc::Rc};

use libadwaita::{Clamp, ViewStack, prelude::WidgetExt};

use super::VIEW_STACK_MAIN_HEADER;

/// Encapsulates common logic for navigating back to a main grid view (Albums or Artists).
///
/// This function performs several UI updates:
/// 1. Sets the `left_btn_stack` (header's left button area) back to the main header view.
/// 2. Makes the `right_btn_box` (header's right button area) visible.
/// 3. Triggers a refresh of the library UI, applying the current sort order.
///
/// This reduces code duplication in `handle_esc_navigation` and `connect_tab_navigation`.
///
/// # Arguments
/// * `stack` - The main `ViewStack` of the application. (Currently unused, but kept for consistency if needed later)
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
pub fn navigate_back_to_main_grid(
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    refresh_library_ui: &Rc<dyn Fn(bool, bool)>,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
) {
    left_btn_stack.set_visible_child_name(VIEW_STACK_MAIN_HEADER);
    right_btn_box.set_visible(true);

    // Refresh the UI with the current sort settings for albums and artists.
    refresh_library_ui(sort_ascending.get(), sort_ascending_artists.get());
}

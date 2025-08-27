use std::{cell::Cell, rc::Rc};

use glib::object::ObjectExt;
use gtk4::{Button, ToggleButton};
use libadwaita::{
    ViewStack,
    prelude::{ButtonExt, ToggleButtonExt},
};

/// Helper function to refresh the UI for the currently active tab.
///
/// This function is a utility to encapsulate the logic for refreshing the library
/// user interface based on the active tab (albums or artists) and their respective
/// sort ascending/descending states. It calls the provided `refresh_library_ui`
/// callback with the appropriate sorting parameters.
///
/// # Arguments
///
/// * `refresh_library_ui` - A reference to an `Rc<dyn Fn(bool, bool)>` closure
///   that triggers a UI refresh. The first boolean indicates album sort direction,
///   the second indicates artist sort direction.
/// * `sort_ascending` - A reference to an `Rc<Cell<bool>>` holding the current
///   sort direction for albums (true for ascending, false for descending).
/// * `sort_ascending_artists` - A reference to an `Rc<Cell<bool>>` holding the current
///   sort direction for artists (true for ascending, false for descending).
fn refresh_ui_for_active_tab(
    refresh_library_ui: &Rc<dyn Fn(bool, bool)>,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
) {
    (refresh_library_ui)(sort_ascending.get(), sort_ascending_artists.get());
}

/// Connects toggled handlers for albums and artists tab buttons to refresh sorting.
///
/// This function sets up event handlers for the album and artist toggle buttons.
/// When a button is toggled to the active state, it triggers a UI refresh, ensuring
/// that the displayed content reflects the current sorting preferences for that tab.
///
/// # Arguments
///
/// * `albums_btn` - The `ToggleButton` for the albums tab.
/// * `artists_btn` - The `ToggleButton` for the artists tab.
/// * `refresh_library_ui` - An `Rc<dyn Fn(bool, bool)>` closure to refresh the main library UI.
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the sort direction for artists.
#[allow(clippy::too_many_arguments)]
pub fn connect_tab_sort_refresh(
    albums_btn: &ToggleButton,
    artists_btn: &ToggleButton,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    stack: Rc<ViewStack>,
) {
    let refresh_library_ui_albums = refresh_library_ui.clone();
    let sort_ascending_albums = sort_ascending.clone();
    let sort_ascending_artists_albums = sort_ascending_artists.clone();
    let stack_albums_clone = stack.clone();
    albums_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            // Only refresh if the albums view is not already the visible child
            if stack_albums_clone.visible_child_name().unwrap_or_default() == "albums" {
                return;
            }
            refresh_ui_for_active_tab(
                &refresh_library_ui_albums,
                &sort_ascending_albums,
                &sort_ascending_artists_albums,
            );
        }
    });

    let refresh_library_ui_artists = refresh_library_ui.clone();
    let sort_ascending_artists_btn = sort_ascending.clone();
    let sort_ascending_artists_val = sort_ascending_artists.clone();
    let stack_artists_clone = stack.clone();
    artists_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            // Only refresh if the artists view is not already the visible child
            if stack_artists_clone.visible_child_name().unwrap_or_default() == "artists" {
                return;
            }
            refresh_ui_for_active_tab(
                &refresh_library_ui_artists,
                &sort_ascending_artists_btn,
                &sort_ascending_artists_val,
            );
        }
    });
}

/// Determines the appropriate sort icon name based on the current page and sort order.
///
/// This helper function returns the symbolic icon name for the sort button,
/// choosing between "view-sort-descending-symbolic" and "view-sort-ascending-symbolic"
/// based on the `page` (e.g., "artists" or "albums") and the corresponding
/// `sort_ascending` state for that page.
///
/// # Arguments
///
/// * `page` - A string slice representing the current visible page name (e.g., "artists", "albums").
/// * `sort_ascending` - A reference to an `Rc<Cell<bool>>` indicating the sort direction for albums.
/// * `sort_ascending_artists` - A reference to an `Rc<Cell<bool>>` indicating the sort direction for artists.
///
/// # Returns
///
/// A `&'static str` containing the appropriate symbolic icon name.
pub fn get_sort_icon_name(
    page: &str,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
) -> &'static str {
    // 1. Select the correct boolean value based on the page.
    let ascending = if page == "artists" {
        sort_ascending_artists.get()
    } else {
        sort_ascending.get()
    };

    // 2. Use that boolean in the now-deduplicated logic.
    if ascending {
        "view-sort-descending-symbolic"
    } else {
        "view-sort-ascending-symbolic"
    }
}

/// Connects a handler to update the sort icon on tab switch.
///
/// This function listens for changes to the `visible-child-name` property of the
/// provided `ViewStack`. When the active tab changes (e.g., between "albums" and "artists"),
/// it updates the `sort_button`'s icon to reflect the correct sort direction
/// for the newly active tab.
///
/// # Arguments
///
/// * `sort_button` - The GTK `Button` widget used to display the sort icon.
/// * `stack` - The `libadwaita::ViewStack` that manages the different library tabs.
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the current sort direction for artists.
#[allow(clippy::too_many_arguments)]
pub fn connect_sort_icon_update_on_tab_switch(
    sort_button: &Button,
    stack: &ViewStack,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    let sort_button = sort_button.clone();
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
        let page = stack.visible_child_name().unwrap_or_default();
        let icon_name = get_sort_icon_name(&page, &sort_ascending, &sort_ascending_artists);
        sort_button.set_icon_name(icon_name);
    });
}

/// Sets the initial sort icon state for a given sort button.
///
/// This function determines the correct icon (ascending or descending) based on
/// the `initial_page` (e.g., "albums" or "artists") and the corresponding
/// `sort_ascending` or `sort_ascending_artists` state. It then sets the icon
/// on the provided `sort_button`. It reuses the `get_sort_icon_name` helper
/// to avoid code duplication.
///
/// # Arguments
///
/// * `sort_button` - The GTK `Button` widget whose icon needs to be set.
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the sort direction for artists.
/// * `initial_page` - A string slice representing the initial active page ("albums" or "artists").
pub fn set_initial_sort_icon_state(
    sort_button: &impl ButtonExt,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
    initial_page: &str,
) {
    let icon_name = get_sort_icon_name(initial_page, sort_ascending, sort_ascending_artists);
    sort_button.set_icon_name(icon_name);
}

use std::{cell::Cell, rc::Rc};

use gtk4::Button;
use libadwaita::{
    ViewStack,
    prelude::{ButtonExt, ObjectExt},
};

use crate::ui::components::{
    config::{load_settings, save_settings},
    sorting::sorting_ui_utils::get_sort_icon_name,
};

use super::{VIEW_STACK_ALBUMS, VIEW_STACK_ARTISTS};

/// Connects the sort button to toggle the sort order (ascending/descending)
/// for the currently visible library view (albums or artists) and updates the UI accordingly.
///
/// This function handles two main aspects:
/// 1. **Button Click Event**: When the sort button is clicked, it determines the current active
///    view (`albums` or `artists`), toggles the corresponding sort order flag (`sort_ascending`
///    or `sort_ascending_artists`), persists this change to user settings, updates the sort
///    button's icon, and triggers a UI refresh.
/// 2. **ViewStack Change Notification**: It also connects to the `visible-child-name` property
///    of the main `ViewStack` to automatically update the sort button's icon when the user
///    switches between the albums and artists views, ensuring the icon always reflects the
///    correct sort state for the active view.
///
/// # Arguments
/// * `sort_button` - The GTK `Button` widget used to trigger sorting.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `sort_ascending` - `Rc<Cell<bool>>` representing the ascending/descending state for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` representing the ascending/descending state for artists.
/// * `refresh_library_ui` - A closure that refreshes the main library UI (albums/artists grid)
///   with the updated sort order.
#[allow(clippy::too_many_arguments)]
pub fn connect_sort_button(
    sort_button: &Button,
    stack: &ViewStack,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    // Clone necessary `Rc`s for use within the closures to extend their lifetime.
    let sort_button_clone = sort_button.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let stack_clone = stack.clone();

    // Connect handler for the sort button's `clicked` signal.
    sort_button.connect_clicked(move |_| {
        let mut settings = load_settings(); // Load current user settings.
        let page = stack_clone.visible_child_name().unwrap_or_default(); // Get current visible page name.
        let current_sort_ascending: bool;
        let current_sort_ascending_artists: bool;

        // Determine which sort state to toggle based on the current page.
        if page == VIEW_STACK_ALBUMS {
            let asc = !sort_ascending_clone.get(); // Toggle the album sort order.
            sort_ascending_clone.set(asc);
            settings.sort_ascending_albums = asc; // Persist the new state.
            current_sort_ascending = asc;
            current_sort_ascending_artists = sort_ascending_artists_clone.get(); // Keep artists sort state as is.
        } else if page == VIEW_STACK_ARTISTS {
            let asc = !sort_ascending_artists_clone.get(); // Toggle the artist sort order.
            sort_ascending_artists_clone.set(asc);
            settings.sort_ascending_artists = asc; // Persist the new state.
            current_sort_ascending = sort_ascending_clone.get(); // Keep album sort state as is.
            current_sort_ascending_artists = asc;
        } else {
            // If neither albums nor artists page is active, do nothing.
            return;
        }
        let _ = save_settings(&settings); // Attempt to save the updated settings.

        // Update the sort button's icon to reflect the new sort state using the helper function.
        let icon_name =
            get_sort_icon_name(&page, &sort_ascending_clone, &sort_ascending_artists_clone);
        sort_button_clone.set_icon_name(icon_name);

        // Trigger a refresh of the library UI with the updated sort orders.
        refresh_library_ui_clone(current_sort_ascending, current_sort_ascending_artists);
    });

    // Connect handler to update the sort icon when the `visible-child-name` property of the `ViewStack` changes.
    let sort_button_for_notify = sort_button.clone();
    let sort_ascending_for_notify = sort_ascending.clone();
    let sort_ascending_artists_for_notify = sort_ascending_artists.clone();
    stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
        let page = stack.visible_child_name().unwrap_or_default();
        // Use the helper function to determine the icon name.
        let icon_name = get_sort_icon_name(
            &page,
            &sort_ascending_for_notify,
            &sort_ascending_artists_for_notify,
        );
        sort_button_for_notify.set_icon_name(icon_name);
    });
}

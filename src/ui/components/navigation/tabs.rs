use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{Button, ToggleButton};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{ButtonExt, ToggleButtonExt},
};

use super::{
    VIEW_STACK_ALBUM_DETAIL, VIEW_STACK_ALBUMS, VIEW_STACK_ARTIST_DETAIL, VIEW_STACK_ARTISTS,
    core::navigate_back_to_main_grid,
};

/// Connects the "Albums" and "Artists" tab toggle buttons to manage `ViewStack` visibility,
/// update the last active tab, and trigger UI refreshes.
///
/// This function sets up `clicked` signal handlers for both tab buttons. When a tab is clicked:
/// 1. It checks if the user is currently on a detail page (album or artist detail) and clears
///    the navigation history if so, ensuring that the back button will return to the newly
///    selected main tab.
/// 2. It updates `last_tab` to remember the currently active main tab.
/// 3. It sets the main `ViewStack` to display the corresponding albums or artists grid.
/// 4. It calls `navigate_back_to_main_grid` to reset the header and refresh the UI.
/// 5. It updates the sort button's icon to reflect the sort state of the newly active tab.
/// 6. It ensures the correct toggle button is active and the other is inactive.
///
/// # Arguments
/// * `albums_btn` - The `ToggleButton` for the "Albums" tab.
/// * `artists_btn` - The `ToggleButton` for the "Artists" tab.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `sort_button` - The `Button` used for sorting, whose icon needs updating.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `last_tab` - `Rc<Cell<&'static str>>` storing the name of the last active main tab.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `rebuild_artist_grid_opt` - An optional closure to rebuild the artists grid. This is used
///   if the artists grid hasn't been built yet (e.g., first time visiting artists tab).
pub fn connect_tab_navigation(
    albums_btn: &ToggleButton,
    artists_btn: &ToggleButton,
    stack: &ViewStack,
    sort_button: &Button,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    last_tab: Rc<Cell<&'static str>>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    rebuild_artist_grid_opt: Option<impl Fn() + 'static>,
) {
    // --- Albums button logic ---
    // Clone all necessary Rc's for the albums button's closure.
    let stack_albums_clone = stack.clone();
    let sort_ascending_albums_clone = sort_ascending.clone();
    let sort_ascending_artists_albums_clone = sort_ascending_artists.clone();
    let refresh_library_ui_albums_clone = refresh_library_ui.clone();
    let sort_button_albums_clone = sort_button.clone();
    let last_tab_albums_clone = last_tab.clone();
    let albums_btn_albums_clone = albums_btn.clone();
    let artists_btn_albums_clone = artists_btn.clone();
    let left_btn_stack_albums_clone = left_btn_stack.clone();
    let right_btn_box_albums_clone = right_btn_box.clone();
    let nav_history_albums_clone = nav_history.clone();
    albums_btn.connect_clicked(move |_| {
        let current_visible_child = stack_albums_clone.visible_child_name().unwrap_or_default();

        // If the albums grid is already the visible child, do nothing to prevent unnecessary refreshes.
        if current_visible_child == VIEW_STACK_ALBUMS {
            return;
        }

        // If currently on a detail page, clear history. This ensures that when the user
        // navigates back from a detail page, they return to the correct main tab (Albums).
        if current_visible_child == VIEW_STACK_ALBUM_DETAIL
            || current_visible_child == VIEW_STACK_ARTIST_DETAIL
        {
            nav_history_albums_clone.borrow_mut().clear();
        }

        // Update the last active tab and set the main ViewStack to the albums view.
        last_tab_albums_clone.set(VIEW_STACK_ALBUMS);
        stack_albums_clone.set_visible_child_name(VIEW_STACK_ALBUMS);

        // Reset the header to the main view and refresh the UI.
        navigate_back_to_main_grid(
            &left_btn_stack_albums_clone,
            &right_btn_box_albums_clone,
            &refresh_library_ui_albums_clone,
            &sort_ascending_albums_clone,
            &sort_ascending_artists_albums_clone,
        );

        // Restore the last used (or persistent) sort direction for albums and update the sort button icon.
        let ascending = sort_ascending_albums_clone.get();
        sort_button_albums_clone.set_icon_name(if ascending {
            "view-sort-descending-symbolic"
        } else {
            "view-sort-ascending-symbolic"
        });

        // Trigger a refresh for the albums view with its specific sort order.
        refresh_library_ui_albums_clone(ascending, sort_ascending_artists_albums_clone.get());

        // Ensure toggle button states are correct: Albums active, Artists inactive.
        albums_btn_albums_clone.set_active(true);
        artists_btn_albums_clone.set_active(false);
    });

    // --- Artists button logic ---
    // Clone all necessary Rc's for the artists button's closure.
    let stack_artists_clone = stack.clone();
    let sort_ascending_artists_clone = sort_ascending.clone();
    let sort_ascending_artists_artists_clone = sort_ascending_artists.clone();
    let refresh_library_ui_artists_clone = refresh_library_ui.clone();
    let sort_button_artists_clone = sort_button.clone();
    let last_tab_artists_clone = last_tab.clone();
    let albums_btn_artists_clone = albums_btn.clone();
    let artists_btn_artists_clone = artists_btn.clone();
    let left_btn_stack_artists_clone = left_btn_stack.clone();
    let right_btn_box_artists_clone = right_btn_box.clone();
    let nav_history_artists_clone = nav_history.clone();
    artists_btn.connect_clicked(move |_| {
        let current_visible_child = stack_artists_clone.visible_child_name().unwrap_or_default();

        // If the artists grid is already the visible child, do nothing to prevent unnecessary refreshes.
        if current_visible_child == VIEW_STACK_ARTISTS {
            return;
        }

        // If currently on a detail page, clear history. Similar to albums tab, this ensures
        // back navigation from a detail page returns to the main Artists tab.
        if current_visible_child == VIEW_STACK_ALBUM_DETAIL
            || current_visible_child == VIEW_STACK_ARTIST_DETAIL
        {
            nav_history_artists_clone.borrow_mut().clear();
        }

        // Update the last active tab.
        last_tab_artists_clone.set(VIEW_STACK_ARTISTS);

        // Check if the Artists view is already present in the ViewStack.
        // If not, and a `rebuild_artist_grid_opt` closure is provided, call it to build the grid.
        // Step 1: If the child doesn't exist, try to build it.
        if stack_artists_clone
            .child_by_name(VIEW_STACK_ARTISTS)
            .is_none()
        {
            if let Some(ref rebuild_artist_grid) = rebuild_artist_grid_opt {
                rebuild_artist_grid();
            }
        }

        // Step 2: Unconditionally try to switch to the child.
        // If the build step failed or wasn't possible, this does nothing, which is safe.
        stack_artists_clone.set_visible_child_name(VIEW_STACK_ARTISTS);

        // Reset the header to the main view and refresh the UI.
        navigate_back_to_main_grid(
            &left_btn_stack_artists_clone,
            &right_btn_box_artists_clone,
            &refresh_library_ui_artists_clone,
            &sort_ascending_artists_clone,
            &sort_ascending_artists_artists_clone,
        );

        // Restore the last used (or persistent) sort direction for artists and update the sort button icon.
        let ascending = sort_ascending_artists_artists_clone.get();
        sort_button_artists_clone.set_icon_name(if ascending {
            "view-sort-descending-symbolic"
        } else {
            "view-sort-ascending-symbolic"
        });

        // Trigger a refresh for the artists view with its specific sort order.
        refresh_library_ui_artists_clone(sort_ascending_artists_clone.get(), ascending);

        // Ensure toggle button states are correct: Artists active, Albums inactive.
        albums_btn_artists_clone.set_active(false);
        artists_btn_artists_clone.set_active(true);
    });
}

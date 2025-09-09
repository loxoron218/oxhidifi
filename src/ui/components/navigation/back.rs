use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{Button, prelude::ButtonExt};
use libadwaita::{Clamp, ViewStack};

use super::{VIEW_STACK_ALBUMS, VIEW_STACK_ARTISTS, utils::navigate_back_to_main_grid};

/// Connects the back button's `clicked` signal to trigger back navigation.
///
/// This function reuses the `handle_back_navigation` closure, ensuring consistent
/// behavior whether the back button is clicked or the Escape key is pressed.
///
/// # Arguments
/// * `back_button` - The GTK `Button` acting as the back button.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `last_tab` - `Rc<Cell<&'static str>>` storing the name of the last active main tab.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
pub fn connect_back_button(
    back_button: &Button,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    last_tab: Rc<Cell<&'static str>>,
    nav_history: Rc<RefCell<Vec<String>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    // Create the navigation closure that will be executed when the back button is clicked.
    let back_nav_action = handle_back_navigation(
        stack.clone(),
        left_btn_stack.clone(),
        right_btn_box.clone(),
        last_tab.clone(),
        nav_history.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );

    // Connect the closure to the back button's `clicked` signal.
    back_button.connect_clicked(move |_| {
        back_nav_action();
    });
}

/// Provides a reusable closure for handling back navigation logic,
/// typically triggered by the back button or Escape key.
///
/// This function determines the previous page from `nav_history`. If history is available,
/// it navigates back to the previous page. If history is empty and the current page is
/// not a main grid (albums/artists), it navigates to the `last_tab`.
/// It also handles resetting header visibility and refreshing the UI when returning to a main grid.
///
/// # Arguments
/// * `stack` - The main `ViewStack` managing application pages.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `last_tab` - `Rc<Cell<&'static str>>` storing the name of the last active main tab.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
///
/// # Returns
/// An `impl Fn()` closure that encapsulates the back navigation logic.
pub fn handle_back_navigation(
    stack: ViewStack,
    left_btn_stack: ViewStack,
    right_btn_box: Clamp,
    last_tab: Rc<Cell<&'static str>>,
    nav_history: Rc<RefCell<Vec<String>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) -> impl Fn() {
    move || {
        // Attempt to pop the previous page from the navigation history.
        if let Some(prev_page) = nav_history.borrow_mut().pop() {
            stack.set_visible_child_name(&prev_page);
            match prev_page.as_str() {
                // The `|` operator creates a pattern that matches either constant.
                VIEW_STACK_ALBUMS | VIEW_STACK_ARTISTS => {
                    // Navigating back to a main grid view, so reset header and refresh UI
                    navigate_back_to_main_grid(
                        &left_btn_stack,
                        &right_btn_box,
                        &refresh_library_ui,
                        &sort_ascending,
                        &sort_ascending_artists,
                    );
                }

                // This arm explicitly does nothing for any other page values.
                _ => {}
            }

            // If not on a main grid, navigate to the last remembered tab and reset header.
        } else {
            // If history is empty, navigate to the last remembered tab and reset header.
            // Get the name of the last active tab.
            let tab = last_tab.get();
            stack.set_visible_child_name(tab);

            // Navigating back to a main grid view, so reset header and refresh UI
            navigate_back_to_main_grid(
                &left_btn_stack,
                &right_btn_box,
                &refresh_library_ui,
                &sort_ascending,
                &sort_ascending_artists,
            );
        }
    }
}

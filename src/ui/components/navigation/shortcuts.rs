use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use glib::Propagation::Stop;
use gtk4::{CallbackAction, KeyvalTrigger, Shortcut, ShortcutController};
use libadwaita::{
    ApplicationWindow, Clamp, ViewStack,
    gdk::{Key, ModifierType},
    prelude::WidgetExt,
};

use crate::ui::search_bar::SearchBar;

use super::core::handle_back_navigation;

/// Sets up keyboard shortcuts for the main application window.
///
/// Currently, this function primarily configures the behavior of the Escape key:
/// - If the search bar is currently revealed, pressing Escape will hide it.
/// - Otherwise, pressing Escape will trigger the standard back navigation logic,
///   moving back through the `nav_history` or to the last active tab.
///
/// # Arguments
/// * `window` - The `ApplicationWindow` to which the shortcuts will be added.
/// * `search_bar` - A reference to the `SearchBar` widget, used to control its visibility.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `last_tab` - `Rc<Cell<&'static str>>` storing the name of the last active main tab.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
pub fn setup_keyboard_shortcuts(
    window: &ApplicationWindow,
    search_bar: &SearchBar,
    refresh_library_ui: &Rc<dyn Fn(bool, bool)>,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    last_tab: &Rc<Cell<&'static str>>,
    nav_history: &Rc<RefCell<Vec<String>>>,
) {
    let accel_group = ShortcutController::new();

    // Clones for the search bar related actions within the Escape key closure.
    let refresh_library_ui_for_search = refresh_library_ui.clone();
    let sort_ascending_for_search = sort_ascending.clone();
    let sort_ascending_artists_for_search = sort_ascending_artists.clone();
    let search_revealer = search_bar.revealer.clone();
    let search_button = search_bar.button.clone();

    // Create the back navigation action, which will be reused by the Escape key.
    // This leverages the shared logic from `core` to ensure consistency.
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

    // Define the Escape key shortcut.
    let esc_shortcut = Shortcut::builder()
        .trigger(&KeyvalTrigger::new(Key::Escape, ModifierType::empty()))
        .action(&CallbackAction::new(move |_, _| {
            // Check if the search bar is currently visible.
            if search_revealer.reveals_child() {
                // If search bar is open, close it and refresh the UI.
                search_revealer.set_reveal_child(false);
                search_button.set_visible(true);
                refresh_library_ui_for_search(
                    sort_ascending_for_search.get(),
                    sort_ascending_artists_for_search.get(),
                );
            } else {
                // If search bar is not open, execute the general back navigation logic.
                back_nav_action();
            }

            // Stop event propagation as the shortcut has been handled in either case.
            Stop
        }))
        .build();

    // Add the Escape key shortcut to the shortcut controller.
    accel_group.add_shortcut(esc_shortcut);
    // Add the shortcut controller to the application window.
    window.add_controller(accel_group);
}

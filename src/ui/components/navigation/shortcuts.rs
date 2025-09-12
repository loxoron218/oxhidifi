use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use glib::Propagation::Stop;
use gtk4::{CallbackAction, KeyvalTrigger, Shortcut, ShortcutController};
use libadwaita::{
    ApplicationWindow, Clamp, ViewStack,
    gdk::{Key, ModifierType},
    prelude::{EditableExt, ObjectExt, WidgetExt},
};

use crate::ui::{
    components::view_controls::{
        ZoomManager,
        list_view::column_view::zoom_manager::ColumnViewZoomManager,
        view_mode::ViewMode::{self, GridView, ListView},
    },
    search_bar::SearchBar,
};

use super::{VIEW_STACK_ALBUMS, VIEW_STACK_ARTISTS, back::handle_back_navigation};

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
/// * `zoom_manager` - `Rc<ZoomManager>` for handling zoom level changes.
/// * `current_view_mode` - `Rc<Cell<ViewMode>>` storing the current view mode.
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
    zoom_manager: &Rc<ZoomManager>,
    column_view_zoom_manager: Option<Rc<ColumnViewZoomManager>>,
    current_view_mode: Rc<Cell<ViewMode>>,
) {
    let accel_group = ShortcutController::new();

    // Clones for the search bar related actions within the Escape key closure.
    let search_button = search_bar.button.clone();
    let search_entry = search_bar.entry.clone();

    // Downgrade references to weak references for use in closures
    let stack_weak = stack.downgrade();

    // Create the back navigation action, which will be reused by the Escape key.
    // This leverages the shared logic from `back` module to ensure consistency.
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
            if search_entry.is_visible() {
                // If search bar is open, close it.
                // Note: We don't call refresh_library_ui here because the search implementation
                // in the search module already handles refreshing when the search text is cleared.
                search_entry.set_visible(false);
                search_button.set_visible(true);

                // Clear the search text, which will trigger the search refresh automatically
                search_entry.set_text("");
            } else {
                // If search bar is not open, check if we're on a main grid view
                // Upgrade the weak reference to check the current visible page
                if let Some(stack) = stack_weak.upgrade() {
                    let current_page = stack.visible_child_name();

                    // Only execute back navigation if we're not already on a main grid view
                    // (albums or artists). Pressing ESC on main grids should do nothing.
                    if let Some(page_name) = current_page {
                        if page_name != VIEW_STACK_ALBUMS && page_name != VIEW_STACK_ARTISTS {
                            back_nav_action();
                        }
                    } else {
                        // If we can't determine the current page, execute back navigation for safety
                        back_nav_action();
                    }
                } else {
                    // If we can't upgrade the weak reference, execute back navigation for safety
                    back_nav_action();
                }
            }

            // Stop event propagation as the shortcut has been handled in either case.
            Stop
        }))
        .build();

    // Add the Escape key shortcut to the shortcut controller.
    accel_group.add_shortcut(esc_shortcut);

    // Add the shortcut controller to the application window.
    window.add_controller(accel_group);

    // Create a new shortcut controller for zoom shortcuts
    let zoom_accel_group = ShortcutController::new();

    // Clone the zoom managers for use in the closures
    let zoom_manager_clone = zoom_manager.clone();
    let column_view_zoom_manager_clone = column_view_zoom_manager.clone();

    // Define the zoom in shortcut (Ctrl + +)
    let current_view_mode_clone = current_view_mode.clone();
    let zoom_in_shortcut = Shortcut::builder()
        .trigger(&KeyvalTrigger::new(Key::plus, ModifierType::CONTROL_MASK))
        .action(&CallbackAction::new(move |_, _| {
            // Apply zoom in only to the current view mode's zoom manager
            match current_view_mode_clone.get() {
                GridView => zoom_manager_clone.zoom_in(),
                ListView => {
                    if let Some(ref column_view_zoom_manager) = column_view_zoom_manager_clone {
                        column_view_zoom_manager.zoom_in();
                    }
                }
            }
            Stop
        }))
        .build();
    zoom_accel_group.add_shortcut(zoom_in_shortcut);

    // Clone the zoom managers for use in the closures
    let zoom_manager_clone = zoom_manager.clone();
    let column_view_zoom_manager_clone = column_view_zoom_manager.clone();
    let current_view_mode = current_view_mode.clone();

    // Define the zoom out shortcut (Ctrl + -)
    let current_view_mode_clone = current_view_mode.clone();
    let zoom_out_shortcut = Shortcut::builder()
        .trigger(&KeyvalTrigger::new(Key::minus, ModifierType::CONTROL_MASK))
        .action(&CallbackAction::new(move |_, _| {
            // Apply zoom out only to the current view mode's zoom manager
            match current_view_mode_clone.get() {
                GridView => zoom_manager_clone.zoom_out(),
                ListView => {
                    if let Some(ref column_view_zoom_manager) = column_view_zoom_manager_clone {
                        column_view_zoom_manager.zoom_out();
                    }
                }
            }
            Stop
        }))
        .build();
    zoom_accel_group.add_shortcut(zoom_out_shortcut);

    // Clone the zoom managers for use in the closures
    let zoom_manager_clone = zoom_manager.clone();
    let column_view_zoom_manager_clone = column_view_zoom_manager.clone();
    let current_view_mode = current_view_mode.clone();

    // Define the reset zoom shortcut (Ctrl + 0)
    let current_view_mode_clone = current_view_mode.clone();
    let reset_zoom_shortcut = Shortcut::builder()
        .trigger(&KeyvalTrigger::new(Key::_0, ModifierType::CONTROL_MASK))
        .action(&CallbackAction::new(move |_, _| {
            // Reset zoom only for the current view mode's zoom manager
            match current_view_mode_clone.get() {
                GridView => zoom_manager_clone.reset_zoom(),
                ListView => {
                    if let Some(ref column_view_zoom_manager) = column_view_zoom_manager_clone {
                        column_view_zoom_manager.reset_zoom();
                    }
                }
            }
            Stop
        }))
        .build();
    zoom_accel_group.add_shortcut(reset_zoom_shortcut);

    // Add the zoom shortcut controller to the application window
    window.add_controller(zoom_accel_group);
}

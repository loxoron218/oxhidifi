use std::rc::Rc;

use gtk4::Box;

use crate::ui::{
    components::navigation::shortcuts::setup_keyboard_shortcuts,
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
};

/// Sets up global keyboard shortcuts for the main application window.
///
/// This function configures keyboard shortcuts that enhance user accessibility,
/// particularly focusing on the Escape key for navigation and search functionality.
/// It delegates the actual shortcut setup to the `setup_keyboard_shortcuts` function
/// in the navigation components module.
///
/// # Arguments
///
/// * `widgets` - A reference to `WindowWidgets` containing all UI components
/// * `shared_state` - A reference to `WindowSharedState` containing shared application state
/// * `refresh_library_ui` - A closure for refreshing the main library UI with sort parameters
/// * `_vbox_inner` - A reference to the inner GTK Box (currently unused but kept for API consistency)
///
/// # Implementation Details
///
/// The function passes various UI components and state references to the underlying
/// `setup_keyboard_shortcuts` function, which handles the actual GTK keyboard shortcut
/// configuration. This includes:
/// - The main application window for attaching shortcut controllers
/// - The search bar components for visibility toggling
/// - Sort state references for UI refreshes
/// - Navigation stack components for page transitions
/// - History tracking for back navigation
pub fn setup_keyboard_shortcuts_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    _vbox_inner: &Box,
) {
    setup_keyboard_shortcuts(
        &widgets.window,
        &widgets.search_bar.search_bar,
        &refresh_library_ui,
        &shared_state.sort_ascending,
        &shared_state.sort_ascending_artists,
        &widgets.stack,
        &widgets.left_btn_stack,
        &widgets.right_btn_box,
        &shared_state.last_tab,
        &shared_state.nav_history,
    );
}

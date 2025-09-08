use std::{rc::Rc, sync::Arc};

use gtk4::Box;
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
    search::connect_live_search,
};

/// Connects the live search functionality to the search entry widget.
///
/// This function sets up real-time search capabilities that trigger as the user types
/// in the search bar. It dynamically updates the album and artist grids with matching
/// results from the database. The search functionality is only connected if all
/// necessary UI components (grids and stacks) are available.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window widgets containing the search bar and grids
/// * `shared_state` - Shared application state including sorting preferences and navigation history
/// * `db_pool` - Database connection pool for executing search queries
/// * `sender` - Channel sender for communication between UI components
/// * `refresh_library_ui` - Closure for refreshing the library UI when needed
pub fn connect_live_search_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    // Only connect live search if we have all necessary grid references
    // This ensures we don't attempt to connect search functionality to non-existent UI components
    if let (Some(albums_grid), Some(albums_stack), Some(artist_grid), Some(artists_stack)) = (
        widgets.albums_grid_cell.borrow().as_ref().cloned(),
        widgets.albums_stack_cell.borrow().as_ref().cloned(),
        widgets.artist_grid_cell.borrow().as_ref().cloned(),
        widgets.artists_stack_cell.borrow().as_ref().cloned(),
    ) {
        connect_live_search(
            &widgets.search_bar.search_bar.entry,
            &albums_grid,
            &albums_stack,
            &artist_grid,
            &artists_stack,
            db_pool.clone(),
            shared_state.sort_ascending.clone(),
            shared_state.sort_ascending_artists.clone(),
            refresh_library_ui.clone(),
            Rc::new(widgets.stack.clone()),
            Rc::new(widgets.left_btn_stack.clone()),
            Rc::new(widgets.right_btn_box.clone()),
            shared_state.nav_history.clone(),
            sender.clone(),
            shared_state.show_dr_badges.clone(),
            shared_state.use_original_year.clone(),
            widgets.player_bar.clone(),
        );
    }
}

/// Sets up the search bar UI logic including showing/hiding behavior and focus management.
///
/// This function integrates the search bar's behavior into the main window's UI flow,
/// handling interactions such as showing the search entry when the search button is clicked,
/// hiding it when focus is lost or when clicking outside, and enabling type-to-search
/// functionality for a seamless user experience.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window widgets containing the search bar
/// * `vbox_inner` - The main vertical box container used for detecting clicks outside the search bar
pub fn setup_search_bar_logic(widgets: &WindowWidgets, vbox_inner: &Box) {
    widgets
        .search_bar
        .search_bar
        .setup_logic(&widgets.window, vbox_inner);
}

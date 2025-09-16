use std::{rc::Rc, sync::Arc};

use glib::Object;
use gtk4::{Box, gio::ListStore};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::refresh::RefreshService,
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
    search::{connect_live_search, connect_live_search_list_view},
};

/// Connects the live search functionality to the search entry widget.
///
/// This function sets up real-time search capabilities that trigger as the user types
/// in the search bar. It dynamically updates the album and artist grids/lists with matching
/// results from the database. The search functionality works with both GridView and ListView modes.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window widgets containing the search bar and grids
/// * `shared_state` - Shared application state including sorting preferences and navigation history
/// * `db_pool` - Database connection pool for executing search queries
/// * `sender` - Channel sender for communication between UI components
/// * `refresh_library_ui` - Closure for refreshing the library UI when needed
/// * `refresh_service` - Service containing references to UI components for ListView mode
pub fn connect_live_search_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    refresh_service: Rc<RefreshService>,
) {
    // Check if we're in GridView mode (FlowBox components exist)
    let is_grid_view = widgets.albums_grid_cell.borrow().is_some();
    if is_grid_view {
        // GridView mode - use FlowBox components
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
                shared_state.screen_info.clone(),
                shared_state.current_zoom_level.clone(),
            );
        }
    } else {
        // ListView mode - use ColumnView components
        // Check if we have the necessary ColumnView components
        if let (Some(_), Some(albums_stack), Some(artists_stack)) = (
            refresh_service.column_view_widget.borrow().as_ref(),
            widgets.albums_stack_cell.borrow().as_ref().cloned(),
            widgets.artists_stack_cell.borrow().as_ref().cloned(),
        ) {
            // Get the ColumnView models from the refresh service
            if let Some(albums_model) = refresh_service.get_column_view_model() {
                // For artists, we don't have a separate model in ListView mode
                // We'll use a placeholder ListStore for now
                let artists_model = ListStore::new::<Object>();
                connect_live_search_list_view(
                    &widgets.search_bar.search_bar.entry,
                    albums_model,
                    &albums_stack,
                    artists_model,
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

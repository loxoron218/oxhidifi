use std::{rc::Rc, sync::Arc};

use gtk4::glib::{MainContext, clone::Downgrade};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::{
        config::{load_settings, save_settings},
        navigation::{
            grid::connect_album_navigation, list_view::connect_list_view_album_navigation,
        },
        refresh::RefreshService,
        view_controls::{
            list_view::population::populate_albums_column_view,
            view_mode::ViewMode::{GridView, ListView},
        },
    },
    grids::album_grid_rebuilder::rebuild_albums_grid_for_window,
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
    pages::album::album_page::album_page,
};

/// Connect the view control button to the sorting system
///
/// This function sets up the connection between the UI button and the sorting
/// functionality, allowing users to sort albums by different criteria.
pub fn connect_view_control_sorting(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    widgets.button.connect_sorting(
        shared_state.sort_orders.clone(),
        shared_state.sort_ascending.clone(),
        shared_state.sort_ascending_artists.clone(),
        refresh_library_ui.clone(),
        Rc::new(widgets.stack.clone()),
    );
}

/// Connect the view control button to handle view mode changes
///
/// This function sets up the event handler for when the user changes the view mode
/// between grid view and list view. It handles rebuilding the UI components,
/// setting up navigation, and updating the configuration.
///
/// # Parameters
/// * `widgets` - The window widgets containing UI elements
/// * `shared_state` - Shared application state
/// * `db_pool` - Database connection pool for data operations
/// * `sender` - Channel sender for communication between components
/// * `refresh_library_ui` - Function to refresh the library UI
/// * `refresh_service` - Service for handling UI refresh operations
pub fn connect_view_mode_change_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    refresh_service: Rc<RefreshService>,
) {
    // Clone all necessary values for the closure to ensure they can be moved
    let screen_info_cloned2 = shared_state.screen_info.clone();
    let sort_orders_cloned2 = shared_state.sort_orders.clone();
    let sort_ascending_cloned2 = shared_state.sort_ascending.clone();
    let show_dr_badges_cloned2 = shared_state.show_dr_badges.clone();
    let use_original_year_cloned2 = shared_state.use_original_year.clone();
    let db_pool2 = db_pool.clone();
    let albums_grid_cell_cloned = widgets.albums_grid_cell.clone();
    let albums_stack_cell_cloned = widgets.albums_stack_cell.clone();
    let album_count_label_cloned = widgets.album_count_label.clone();
    let stack_cloned = widgets.stack.clone();
    let scanning_label_albums_cloned = widgets.scanning_label_albums.clone();
    let player_bar_cloned = widgets.player_bar.clone();
    let left_btn_stack_cloned = widgets.left_btn_stack.clone();
    let right_btn_box_cloned = widgets.right_btn_box.clone();
    let window_cloned = widgets.window.clone();
    let button_cloned = widgets.button.clone();
    let sender_cloned = sender.clone();
    let nav_history_cloned2 = shared_state.nav_history.clone();
    let sort_ascending_artists_cloned2 = shared_state.sort_ascending_artists.clone();
    let column_view_zoom_manager_cloned = shared_state.column_view_zoom_manager.clone();

    // Connect the view mode changed signal to handle UI updates
    let current_view_mode_clone = shared_state.current_view_mode.clone();
    button_cloned
        .clone()
        .connect_view_mode_changed(move |view_mode| {
            // Update the current view mode in shared state
            current_view_mode_clone.set(view_mode);

            // Clone the necessary values for the closure to ensure they can be moved into async blocks
            let screen_info_clone = screen_info_cloned2.clone();
            let albums_grid_cell_clone = albums_grid_cell_cloned.clone();
            let albums_stack_cell_clone = albums_stack_cell_cloned.clone();
            let album_count_label_clone = album_count_label_cloned.clone();
            let stack_clone = stack_cloned.clone();
            let scanning_label_albums_clone = scanning_label_albums_cloned.clone();
            let player_bar_clone = player_bar_cloned.clone();
            let left_btn_stack_clone = left_btn_stack_cloned.clone();
            let right_btn_box_clone = right_btn_box_cloned.clone();
            let window_clone = window_cloned.clone();
            let db_pool_clone = db_pool2.clone();
            let sender_clone = sender_cloned.clone();
            let nav_history_clone = nav_history_cloned2.clone();
            let show_dr_badges_clone = show_dr_badges_cloned2.clone();
            let refresh_service_clone = refresh_service.clone();
            let sort_orders_clone = sort_orders_cloned2.clone();
            let sort_ascending_clone = sort_ascending_cloned2.clone();
            let use_original_year_clone = use_original_year_cloned2.clone();

            // Rebuild the albums grid with the new view mode
            // This function creates the appropriate UI components based on the selected view mode
            let model = rebuild_albums_grid_for_window(
                &stack_clone,
                &scanning_label_albums_clone,
                &screen_info_clone,
                &albums_grid_cell_clone,
                &albums_stack_cell_clone,
                &window_clone.into(),
                &db_pool_clone,
                &sender_clone,
                album_count_label_clone.clone(),
                view_mode,
                use_original_year_clone.get(),
                show_dr_badges_clone.clone(),
                Some(refresh_service_clone.clone()),
                Some(column_view_zoom_manager_cloned.clone()),
                refresh_service_clone.image_loader.clone(),
            );

            // Update the button's view mode to match the new view mode
            // This ensures the UI reflects the current state and updates zoom controls
            button_cloned.set_view_mode(view_mode);

            // Connect album navigation based on the view mode
            // Different view modes require different navigation handling
            match view_mode {
                ListView => {
                    // If we're in ListView mode, populate the column view with data
                    if let Some(model) = model {
                        // Set the ColumnView model in the RefreshService
                        // This allows the refresh service to manage the list view model
                        refresh_service_clone.set_column_view_model(Some(model.clone()));

                        // Connect list view album navigation
                        // This sets up click handlers for albums in the list view
                        if let Some(column_view) =
                            refresh_service_clone.column_view_widget.borrow().as_ref()
                        {
                            connect_list_view_album_navigation(
                                column_view,
                                stack_clone.downgrade(),
                                db_pool_clone.clone(),
                                &left_btn_stack_clone,
                                &right_btn_box_clone,
                                nav_history_clone.clone(),
                                sender_clone.clone(),
                                show_dr_badges_clone.clone(),
                                player_bar_clone.clone(),
                                move |stack_weak,
                                      db_pool,
                                      album_id,
                                      left_btn_stack_weak,
                                      right_btn_box_weak,
                                      sender,
                                      show_dr_badges,
                                      player_bar| {
                                    // Create a closure that will be called when an album is clicked
                                    let show_dr_badges_clone_for_closure = show_dr_badges.clone();
                                    let player_bar_clone = player_bar.clone();
                                    async move {
                                        album_page(
                                            stack_weak,
                                            db_pool,
                                            album_id,
                                            left_btn_stack_weak,
                                            right_btn_box_weak,
                                            sender,
                                            show_dr_badges_clone_for_closure.clone(),
                                            player_bar_clone,
                                        )
                                        .await;
                                    }
                                },
                            );
                        }

                        // Clone the necessary values for the async block
                        // These values need to be moved into the async task
                        let db_pool_clone_inner = db_pool_clone.clone();
                        let sort_orders_clone_inner = sort_orders_clone.clone();
                        let sort_ascending_clone_inner = sort_ascending_clone.clone();
                        let use_original_year_clone_inner = use_original_year_clone.clone();
                        let player_bar_clone_inner = player_bar_clone.clone();

                        // Get the albums stack to pass to the population function
                        // The albums stack is needed to properly populate the list view
                        if let Some(albums_stack) = albums_stack_cell_clone.borrow().as_ref() {
                            let albums_stack_clone = albums_stack.clone();
                            let album_count_label_clone_inner = album_count_label_clone.clone();

                            // Spawn the async task to populate the column view
                            // This operation is async because it involves database queries
                            MainContext::default().spawn_local(async move {
                                populate_albums_column_view(
                                    &model,
                                    db_pool_clone_inner,
                                    sort_ascending_clone_inner.get(),
                                    sort_orders_clone_inner,
                                    &albums_stack_clone,
                                    &album_count_label_clone_inner,
                                    use_original_year_clone_inner,
                                    player_bar_clone_inner,
                                )
                                .await;
                            });
                        }
                    }
                }
                GridView => {
                    // If we're in GridView mode, connect the album navigation for the FlowBox
                    // This sets up click handlers for albums in the grid view
                    if let Some(albums_grid) = albums_grid_cell_clone.borrow().as_ref() {
                        connect_album_navigation(
                            albums_grid,
                            &stack_clone,
                            db_pool_clone.clone(),
                            &left_btn_stack_clone,
                            &right_btn_box_clone,
                            nav_history_clone.clone(),
                            sender_clone.clone(),
                            move |stack_weak,
                                  db_pool,
                                  album_id,
                                  left_btn_stack_weak,
                                  right_btn_box_weak,
                                  sender| {
                                // Create a closure that will be called when an album is clicked
                                let show_dr_badges_clone_for_closure = show_dr_badges_clone.clone();
                                let player_bar_clone = player_bar_clone.clone();
                                async move {
                                    album_page(
                                        stack_weak,
                                        db_pool,
                                        album_id,
                                        left_btn_stack_weak,
                                        right_btn_box_weak,
                                        sender,
                                        show_dr_badges_clone_for_closure.clone(),
                                        player_bar_clone,
                                    )
                                    .await;
                                }
                            },
                        );
                    }

                    // If we're not in ListView mode, clear the ColumnView model reference
                    // This ensures the refresh service doesn't try to update a non-existent list view
                    refresh_service_clone.set_column_view_model(None);
                }
            }

            // Trigger a refresh to populate the newly created grid
            // This ensures the UI is updated with the correct data
            refresh_library_ui(
                sort_ascending_clone.get(),
                sort_ascending_artists_cloned2.get(),
            );

            // Save the new view mode to the configuration
            // This persists the user's preference across application restarts
            let mut settings = load_settings();
            settings.view_mode = view_mode;
            let _ = save_settings(&settings);
        });
}

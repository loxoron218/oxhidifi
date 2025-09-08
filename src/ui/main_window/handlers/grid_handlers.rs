use std::{rc::Rc, sync::Arc};

use glib::MainContext;
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::{
        config::load_settings, refresh::RefreshService,
        view_controls::list_view::population::populate_albums_column_view,
    },
    grids::album_grid_rebuilder::rebuild_albums_grid_for_window,
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
};

/// Rebuild and populate initial grids for albums and artists.
/// These functions create the `FlowBox` grids and their containing `Stack`s,
/// and then populate them with initial data or empty states. This ensures the UI
/// is ready to display content as soon as the application launches.
///
/// # Arguments
/// * `widgets` - A reference to `WindowWidgets` containing all UI components for the main window.
/// * `shared_state` - A reference to `WindowSharedState` containing shared application state.
/// * `db_pool` - An `Arc<SqlitePool>` for database access to fetch album/artist data.
/// * `sender` - An `UnboundedSender<()>` for sending UI refresh signals.
/// * `_refresh_library_ui` - A closure for refreshing the library UI (currently unused).
/// * `refresh_service` - An `Rc<RefreshService>` for managing UI refresh operations.
pub fn rebuild_and_populate_grids(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    _refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    refresh_service: Rc<RefreshService>,
) {
    // Load persistent user settings for initial sort orders and view mode.
    // This ensures the UI respects user preferences from previous sessions.
    let settings = load_settings();

    // Clone shared state references for use in the grid rebuilding process.
    // These clones are necessary because we need to move the values into closures.
    let sort_orders_cloned = shared_state.sort_orders.clone();
    let sort_ascending_cloned = shared_state.sort_ascending.clone();
    let screen_info_cloned = shared_state.screen_info.clone();
    let show_dr_badges_cloned = shared_state.show_dr_badges.clone();
    let use_original_year_cloned = shared_state.use_original_year.clone();

    // Rebuild and populate initial grids for albums and artists.
    // This function creates the UI components (FlowBox/ColumnView and Stack) and
    // returns an optional ListStore model for ListView mode.
    let model = rebuild_albums_grid_for_window(
        &widgets.stack,
        &widgets.scanning_label_albums,
        &screen_info_cloned,
        &widgets.albums_grid_cell,
        &widgets.albums_stack_cell,
        &widgets.window.clone().into(),
        &db_pool,
        &sender,
        widgets.album_count_label.clone(),
        settings.view_mode,
        use_original_year_cloned.get(),
        show_dr_badges_cloned.clone(),
        Some(refresh_service.clone()),
    );

    // Update the view control button's mode to match the initial view mode from settings.
    // This ensures the UI button correctly reflects the current view state.
    widgets.button.set_view_mode(settings.view_mode);

    // If we're in ListView mode, populate the column view with data asynchronously.
    // For GridView mode, this step is skipped as the grid is populated elsewhere.
    if let Some(model) = model {
        // Set the ColumnView model in the RefreshService for future refresh operations.
        // This allows the refresh service to update the ListView when needed.
        refresh_service.set_column_view_model(Some(model.clone()));

        // Clone the necessary values for the async block to avoid borrowing issues.
        // These clones are required because the async block needs to own the values.
        let db_pool_clone = db_pool.clone();
        let sort_orders_clone = sort_orders_cloned.clone();
        let sort_ascending_clone = sort_ascending_cloned.clone();
        let use_original_year_clone = use_original_year_cloned.clone();
        let player_bar_clone = widgets.player_bar.clone();

        // Get the albums stack to pass to the population function.
        // This is needed to update the stack's visible child based on loading state.
        if let Some(albums_stack) = widgets.albums_stack_cell.borrow().as_ref() {
            let albums_stack_clone = albums_stack.clone();
            let album_count_label_clone = widgets.album_count_label.clone();

            // Spawn the async task to populate the column view with album data.
            // This operation fetches data from the database and populates the UI without
            // blocking the main thread, ensuring the application remains responsive.
            MainContext::default().spawn_local(async move {
                populate_albums_column_view(
                    &model,
                    db_pool_clone,
                    sort_ascending_clone.get(),
                    sort_orders_clone,
                    &albums_stack_clone,
                    &album_count_label_clone,
                    use_original_year_clone,
                    player_bar_clone,
                )
                .await;
            });
        }
    } else {
        // If we're not in ListView mode (i.e., in GridView mode), clear the ColumnView model reference.
        // This ensures the RefreshService doesn't attempt to update a non-existent ListView.
        refresh_service.set_column_view_model(None);
    }

    // Rebuild and populate initial grid for artists.
    // This function creates the `FlowBox` grid and its containing `Stack` for artists,
    // and then populates them with initial data or empty states. This ensures the UI
    // is ready to display content as soon as the application launches, similar to
    // how the albums grid is initialized above.
    // Note: Both album and artist grids are now handled in the builder.rs file, so we don't need to do it here.
}

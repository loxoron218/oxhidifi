use std::{cell::Cell, rc::Rc, sync::Arc};

use gtk4::Button;
use libadwaita::prelude::ObjectExt;
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::{
        navigation::{
            back::connect_back_button,
            grid::{connect_album_navigation, connect_artist_navigation},
            list_view::connect_list_view_album_navigation,
            tabs::connect_tab_navigation,
        },
        player_bar::PlayerBar,
        refresh::RefreshService,
        view_controls::ZoomLevel,
    },
    grids::artist_grid_rebuilder::rebuild_artist_grid_for_window,
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
    pages::{album::album_page::album_page, artist::artist_page::artist_page},
};

/// Connects the back button functionality to enable navigation to previous views.
///
/// This handler sets up the connection between the UI back button and the core navigation logic.
/// When clicked, the back button uses the navigation history to return to the previous view,
/// providing a consistent navigation experience throughout the application.
///
/// The back button behavior is also accessible via the Escape key through keyboard shortcuts,
/// ensuring consistent navigation whether using mouse or keyboard.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window's UI widgets containing the back button
/// * `shared_state` - Shared application state including navigation history and sort settings
/// * `refresh_library_ui` - Closure for refreshing the library UI with current sort settings
///
/// # Implementation Details
///
/// This function delegates to [`connect_back_button`] in the navigation back module,
/// passing all necessary UI components and shared state. The function handles:
/// - Connecting the button's click signal to navigation logic
/// - Managing navigation history traversal
/// - Updating header visibility when navigating between views
/// - Refreshing UI content with appropriate sort settings
pub fn connect_back_button_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    connect_back_button(
        &widgets.back_button,
        &widgets.stack,
        &widgets.left_btn_stack,
        &widgets.right_btn_box,
        shared_state.last_tab.clone(),
        shared_state.nav_history.clone(),
        refresh_library_ui.clone(),
        shared_state.sort_ascending.clone(),
        shared_state.sort_ascending_artists.clone(),
    );
}

/// Connects tab navigation for switching between Albums and Artists views.
///
/// This handler sets up navigation between the main Albums and Artists views using the
/// tab buttons in the header. When a tab is clicked, it:
/// 1. Updates the main view stack to show the selected tab's content
/// 2. Manages navigation history when switching from detail views
/// 3. Refreshes the UI with appropriate sort settings for the selected tab
/// 4. Rebuilds the artist grid if needed (first time visiting Artists tab)
///
/// # Arguments
///
/// * `widgets` - Reference to the main window's UI widgets containing tab buttons and view stack
/// * `shared_state` - Shared application state including last active tab and navigation history
/// * `refresh_library_ui` - Closure for refreshing the library UI with current sort settings
///
/// # Implementation Details
///
/// This function creates a closure for rebuilding the artist grid when needed, which is
/// used if the user visits the Artists tab for the first time. It then delegates to
/// [`connect_tab_navigation`] in the navigation tabs module, passing all necessary
/// UI components, shared state, and the rebuild closure.
///
/// The tab navigation logic handles:
/// - Click events on Albums and Artists toggle buttons
/// - Navigation history management when leaving detail views
/// - Header state updates (showing/hiding appropriate buttons)
/// - UI refresh with tab-specific sort settings
/// - Artist grid rebuilding for first-time visits to Artists tab
pub fn connect_tab_navigation_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    // Create a closure for rebuilding the artist grid when needed
    // This is used if the user visits the Artists tab for the first time
    let stack_clone = widgets.stack.clone();
    let scanning_label_artists_clone = widgets.scanning_label_artists.clone();
    let artist_grid_cell_clone = widgets.artist_grid_cell.clone();
    let artists_stack_cell_clone = widgets.artists_stack_cell.clone();
    let artist_count_label_clone = widgets.artist_count_label.clone();
    let add_music_button_artists_clone = Button::with_label("Add Music");

    let rebuild_artist_grid_closure = move || {
        rebuild_artist_grid_for_window(
            &stack_clone,
            &scanning_label_artists_clone,
            &artist_grid_cell_clone,
            &artists_stack_cell_clone,
            &add_music_button_artists_clone,
            artist_count_label_clone.clone(),
        );
    };

    connect_tab_navigation(
        &widgets.albums_btn,
        &widgets.artists_btn,
        &widgets.stack,
        &widgets.left_btn_stack,
        &widgets.right_btn_box,
        shared_state.last_tab.clone(),
        shared_state.nav_history.clone(),
        shared_state.sort_ascending.clone(),
        shared_state.sort_ascending_artists.clone(),
        refresh_library_ui.clone(),
        Some(rebuild_artist_grid_closure),
    );
}

/// Connects album navigation for the grid view (clicking on album tiles).
///
/// This handler enables users to navigate to album detail pages by clicking on album tiles
/// in the grid view. When an album tile is clicked, it:
/// 1. Adds the current view to navigation history for back navigation
/// 2. Updates header visibility to show the back button
/// 3. Hides right-side header buttons
/// 4. Asynchronously loads and displays the album detail page
///
/// This handler must be re-connected whenever the albums grid is rebuilt to ensure all
/// dynamically created album tiles remain interactive.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window's UI widgets containing the albums grid
/// * `shared_state` - Shared application state including navigation history
/// * `db_pool` - Database connection pool for fetching album data
/// * `sender` - Channel sender for UI update notifications
/// * `show_dr_badges_cloned` - Shared flag controlling display of DR badges
/// * `player_bar_clone` - Reference to the application's player bar component
///
/// # Implementation Details
///
/// The function first checks if an albums grid exists in the UI. If it does, it creates
/// clones of the necessary shared components and delegates to [`connect_album_navigation`]
/// in the navigation grid module.
///
/// The navigation closure passed to the function is responsible for:
/// - Capturing the album ID from the clicked tile
/// - Spawning an async task to load album data and build the detail page
/// - Calling the [`album_page`] function to construct and display the album detail UI
pub fn connect_album_navigation_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    show_dr_badges_cloned: Rc<Cell<bool>>,
    player_bar_clone: PlayerBar,
) {
    // Only connect navigation if the albums grid exists
    if let Some(albums_grid) = widgets.albums_grid_cell.borrow().as_ref() {
        // Clone shared components for use in the navigation closure
        let show_dr_badges_clone = show_dr_badges_cloned.clone();
        let player_bar_clone = player_bar_clone.clone();

        // Delegate to the grid navigation function with all necessary parameters
        connect_album_navigation(
            albums_grid,
            &widgets.stack,
            db_pool.clone(),
            &widgets.left_btn_stack,
            &widgets.right_btn_box,
            shared_state.nav_history.clone(),
            sender.clone(),
            move |stack_weak,
                  db_pool,
                  album_id,
                  left_btn_stack_weak,
                  right_btn_box_weak,
                  sender| {
                // Clone shared components for the async closure
                let show_dr_badges_clone_for_closure = show_dr_badges_clone.clone();
                let player_bar_clone = player_bar_clone.clone();

                // Return an async block that builds and displays the album detail page
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
}

/// Connects album navigation for the list view.
///
/// This handler enables users to navigate to album detail pages by clicking on albums
/// in the list view. When an album is selected, it:
/// 1. Adds the current view to navigation history for back navigation
/// 2. Updates header visibility to show the back button
/// 3. Hides right-side header buttons
/// 4. Asynchronously loads and displays the album detail page
///
/// This handler is specific to the ListView mode and works with GTK ColumnView widgets.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window's UI widgets
/// * `shared_state` - Shared application state including navigation history
/// * `db_pool` - Database connection pool for fetching album data
/// * `sender` - Channel sender for UI update notifications
/// * `show_dr_badges_cloned` - Shared flag controlling display of DR badges
/// * `player_bar_clone` - Reference to the application's player bar component
/// * `refresh_service` - Service containing references to UI components for ListView mode
///
/// # Implementation Details
///
/// The function first checks if a ColumnView widget exists in the refresh service.
/// If it does, it delegates to [`connect_list_view_album_navigation`] in the navigation
/// list_view module, passing all necessary UI components and a navigation closure.
///
/// The navigation closure is responsible for:
/// - Capturing the album ID from the selected list item
/// - Spawning an async task to load album data and build the detail page
/// - Calling the [`album_page`] function to construct and display the album detail UI
pub fn connect_list_view_album_navigation_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    show_dr_badges_cloned: Rc<Cell<bool>>,
    player_bar_clone: PlayerBar,
    refresh_service: Rc<RefreshService>,
) {
    // Connect list view album navigation only if the ColumnView widget exists
    if let Some(column_view) = refresh_service.column_view_widget.borrow().as_ref() {
        // Delegate to the list view navigation function for list view with all necessary parameters
        connect_list_view_album_navigation(
            column_view,
            widgets.stack.downgrade(),
            db_pool.clone(),
            &widgets.left_btn_stack,
            &widgets.right_btn_box,
            shared_state.nav_history.clone(),
            sender.clone(),
            show_dr_badges_cloned.clone(),
            player_bar_clone.clone(),
            move |stack_weak,
                  db_pool,
                  album_id,
                  left_btn_stack_weak,
                  right_btn_box_weak,
                  sender,
                  show_dr_badges,
                  player_bar| {
                // Clone shared components for the async closure
                let show_dr_badges_clone_for_closure = show_dr_badges.clone();
                let player_bar_clone = player_bar.clone();

                // Return an async block that builds and displays the album detail page
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
}

/// Connects artist navigation (clicking on artist tiles).
///
/// This handler enables users to navigate to artist detail pages by clicking on artist tiles.
/// When an artist tile is clicked, it:
/// 1. Adds the current view to navigation history for back navigation
/// 2. Updates header visibility to show the back button
/// 3. Hides right-side header buttons
/// 4. Asynchronously loads and displays the artist detail page
///
/// This handler must be re-connected whenever the artists grid is rebuilt to ensure all
/// dynamically created artist tiles remain interactive.
///
/// # Arguments
///
/// * `widgets` - Reference to the main window's UI widgets containing the artist grid
/// * `shared_state` - Shared application state including navigation history
/// * `db_pool` - Database connection pool for fetching artist data
/// * `sender` - Channel sender for UI update notifications
/// * `show_dr_badges_cloned` - Shared flag controlling display of DR badges
/// * `use_original_year_cloned` - Shared flag for choosing between original/release years
/// * `player_bar_clone` - Reference to the application's player bar component
///
/// # Implementation Details
///
/// The function first checks if an artist grid exists in the UI. If it does, it creates
/// clones of the necessary shared components and delegates to [`connect_artist_navigation`]
/// in the navigation grid module.
///
/// The navigation closure passed to the function is responsible for:
/// - Capturing the artist ID from the clicked tile
/// - Spawning an async task to load artist data and build the detail page
/// - Calling the [`artist_page`] function to construct and display the artist detail UI
pub fn connect_artist_navigation_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    show_dr_badges_cloned: Rc<Cell<bool>>,
    use_original_year_cloned: Rc<Cell<bool>>,
    player_bar_clone: PlayerBar,
    current_zoom_level: ZoomLevel,
) {
    // Only connect navigation if the artist grid exists
    if let Some(artist_grid) = widgets.artist_grid_cell.borrow().as_ref() {
        // Clone shared components for use in the navigation closure
        let show_dr_badges_clone = show_dr_badges_cloned.clone();
        let use_original_year_clone = use_original_year_cloned.clone();
        let player_bar_clone = player_bar_clone.clone();

        // Clone the necessary fields from shared_state outside the closure
        let screen_info = shared_state.screen_info.clone();

        // Delegate to the grid navigation function with all necessary parameters
        connect_artist_navigation(
            artist_grid,
            &widgets.stack,
            db_pool.clone(),
            &widgets.left_btn_stack,
            &widgets.right_btn_box,
            shared_state.nav_history.clone(),
            sender.clone(),
            show_dr_badges_clone,
            use_original_year_clone,
            player_bar_clone,
            screen_info.clone(),
            move |stack_weak,
                  db_pool,
                  artist_id,
                  left_btn_stack_weak,
                  right_btn_box_weak,
                  nav_history,
                  sender,
                  show_dr_badges,
                  use_original_year,
                  player_bar,
                  screen_info_async| {
                // Return an async block that builds and displays the artist detail page
                async move {
                    artist_page(
                        stack_weak,
                        db_pool,
                        artist_id,
                        left_btn_stack_weak,
                        right_btn_box_weak,
                        nav_history,
                        sender,
                        show_dr_badges,
                        use_original_year,
                        player_bar,
                        screen_info_async,
                        current_zoom_level,
                    )
                    .await;
                }
            },
        );
    }
}

pub mod dialog_handlers;
pub mod grid_handlers;
pub mod keyboard_handlers;
pub mod navigation_handlers;
pub mod refresh_handlers;
pub mod search_handlers;
pub mod view_mode_handlers;

use std::{rc::Rc, sync::Arc};

use gtk4::{Box, Button};
use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::ui::{
    components::refresh::RefreshService,
    main_window::handlers::dialog_handlers::{
        connect_add_folder_handlers, connect_settings_dialog_handler,
    },
    main_window::handlers::{
        grid_handlers::rebuild_and_populate_grids,
        keyboard_handlers::setup_keyboard_shortcuts_handler,
        navigation_handlers::{
            connect_album_navigation_handler, connect_artist_navigation_handler,
            connect_back_button_handler, connect_list_view_album_navigation_handler,
            connect_tab_navigation_handler,
        },
        refresh_handlers::{
            setup_live_monitor_refresh_handler, spawn_scanning_label_refresh_task_handler,
            start_library_watcher,
        },
        search_handlers::{connect_live_search_handler, setup_search_bar_logic},
        view_mode_handlers::{connect_view_control_sorting, connect_view_mode_change_handler},
    },
};

use super::{state::WindowSharedState, widgets::WindowWidgets};

/// Connects all UI event handlers and initializes various components of the main window.
///
/// This function centralizes the setup of all interactions within the main application window,
/// including button clicks, tab navigation, search functionality, and background refresh tasks.
/// It takes references to the `WindowWidgets` and `WindowSharedState` structs, along with
/// other necessary shared resources, to establish the connections.
///
/// The function follows a specific initialization order:
/// 1. Dialog handlers (for settings and folder management)
/// 2. Grid population (albums and artists)
/// 3. Navigation handlers (tabs, back button, album/artist views)
/// 4. Refresh mechanisms (live monitoring, library watching)
/// 5. Search functionality
/// 6. Keyboard shortcuts
/// 7. View controls (sorting and view mode changes)
///
/// # Arguments
///
/// * `widgets` - A reference to the `WindowWidgets` struct containing all the main UI widgets.
/// * `shared_state` - A reference to the `WindowSharedState` struct holding the application's mutable state.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations.
/// * `sender` - An `UnboundedSender<()>` to send signals for UI refreshes.
/// * `receiver` - An `UnboundedReceiver<()>` to receive signals for scan feedback.
/// * `refresh_library_ui` - A closure to trigger a full UI refresh.
/// * `refresh_service` - An `Rc<refresh::RefreshService>` for managing live UI updates.
/// * `vbox_inner` - The main vertical box container.
/// * `add_music_button_albums` - The "Add Music" button for albums.
/// * `add_music_button_artists` - The "Add Music" button for artists.
pub fn connect_all_handlers(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    receiver: UnboundedReceiver<()>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    refresh_service: Rc<RefreshService>,
    vbox_inner: &Box,
    add_music_button_albums: &Button,
    add_music_button_artists: &Button,
) {
    // Connect dialog handlers for folder management and settings
    connect_add_folder_handlers(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        vbox_inner,
        add_music_button_albums,
        add_music_button_artists,
    );

    connect_settings_dialog_handler(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        refresh_library_ui.clone(),
    );

    // Rebuild and populate grids with album and artist data
    // This must be done before connecting navigation handlers that depend on grid content
    rebuild_and_populate_grids(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        refresh_library_ui.clone(),
        refresh_service.clone(),
    );

    // Connect navigation handlers after grids are built
    // Back button handler for navigation history
    connect_back_button_handler(widgets, shared_state, refresh_library_ui.clone());

    // Tab navigation between Albums and Artists views
    connect_tab_navigation_handler(widgets, shared_state, refresh_library_ui.clone());

    // Album navigation from grid view to album detail page
    connect_album_navigation_handler(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        shared_state.show_dr_badges.clone(),
        widgets.player_bar.clone(),
    );

    // Album navigation from list view to album detail page
    connect_list_view_album_navigation_handler(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        shared_state.show_dr_badges.clone(),
        widgets.player_bar.clone(),
        refresh_service.clone(),
    );

    // Artist navigation from grid view to artist detail page
    connect_artist_navigation_handler(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        shared_state.show_dr_badges.clone(),
        shared_state.use_original_year.clone(),
        widgets.player_bar.clone(),
    );

    // Setup live monitor refresh for automatic UI updates
    setup_live_monitor_refresh_handler(shared_state, refresh_service.clone());

    // Start the library watcher to monitor file system changes
    start_library_watcher(db_pool.clone(), sender.clone());

    // Spawn scanning label refresh task to update UI during library scans
    spawn_scanning_label_refresh_task_handler(
        widgets,
        shared_state,
        receiver,
        refresh_library_ui.clone(),
    );

    // Connect live search functionality for real-time filtering
    connect_live_search_handler(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        refresh_library_ui.clone(),
    );

    // Set up search bar UI logic for focus and visibility handling
    setup_search_bar_logic(widgets, vbox_inner);

    // Setup global keyboard shortcuts for application-wide actions
    setup_keyboard_shortcuts_handler(
        widgets,
        shared_state,
        refresh_library_ui.clone(),
        vbox_inner,
    );

    // Connect view control button to sorting system for grid/list organization
    connect_view_control_sorting(widgets, shared_state, refresh_library_ui.clone());

    // Connect view control button to handle view mode changes (grid vs list)
    connect_view_mode_change_handler(
        widgets,
        shared_state,
        db_pool.clone(),
        sender.clone(),
        refresh_library_ui.clone(),
        refresh_service.clone(),
    );
}

use std::{cell::RefCell, rc::Rc, sync::Arc};

use gtk4::{Box, Button};
use libadwaita::prelude::ButtonExt;
use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    data::watcher::start_watching_library,
    ui::{
        components::{
            dialogs::{connect_settings_dialog, create_add_folder_dialog_handler},
            navigation::{
                core::{connect_album_navigation, connect_back_button},
                shortcuts::setup_keyboard_shortcuts,
                sorting::connect_sort_button,
                tabs::connect_tab_navigation,
            },
            refresh::{RefreshService, setup_live_monitor_refresh},
            scan_feedback::spawn_scanning_label_refresh_task,
            sorting::sorting_ui_utils::{
                connect_sort_icon_update_on_tab_switch, connect_tab_sort_refresh,
                set_initial_sort_icon_state,
            },
        },
        grids::{
            album_grid_rebuilder::rebuild_albums_grid_for_window,
            artist_grid_rebuilder::rebuild_artist_grid_for_window,
        },
        pages::album_page::album_page,
        search::connect_live_search,
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
/// # Arguments
///
/// * `widgets` - A reference to the `WindowWidgets` struct containing all the main UI widgets.
/// * `shared_state` - A reference to the `WindowSharedState` struct holding the application's mutable state.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations.
/// * `sender` - An `UnboundedSender<()>` to send signals for UI refreshes.
/// * `receiver` - An `UnboundedReceiver<()>` to receive signals for scan feedback.
/// * `refresh_library_ui` - A closure to trigger a full UI refresh.
/// * `refresh_service` - An `Rc<refresh::RefreshService>` for managing live UI updates.
/// * `screen_info` - The `ScreenInfo` struct containing primary screen dimensions and calculated UI element sizes.
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
    let sort_orders_cloned = shared_state.sort_orders.clone();
    let sort_ascending_cloned = shared_state.sort_ascending.clone();
    let sort_ascending_artists_cloned = shared_state.sort_ascending_artists.clone();
    let last_tab_cloned = shared_state.last_tab.clone();
    let nav_history_cloned = shared_state.nav_history.clone();
    let screen_info_cloned = shared_state.screen_info.clone();
    let is_settings_open_cloned = shared_state.is_settings_open.clone();
    let show_dr_badges_cloned = shared_state.show_dr_badges.clone();
    let use_original_year_cloned = shared_state.use_original_year.clone();
    let view_mode_cloned = shared_state.view_mode.clone();

    // Set initial sort icon state based on loaded settings.
    // This ensures the sort button's icon correctly reflects the default or saved sort order
    // when the application starts.
    set_initial_sort_icon_state(
        &widgets.sort_button,
        &sort_ascending_cloned,
        &sort_ascending_artists_cloned,
        "albums",
    );

    // Connect back button functionality.
    // The back button uses the `nav_history` to navigate to previous views, providing
    // a consistent navigation experience throughout the application.
    connect_back_button(
        &widgets.back_button,
        &widgets.stack,
        &widgets.left_btn_stack,
        &widgets.right_btn_box,
        last_tab_cloned.clone(),
        nav_history_cloned.clone(),
        refresh_library_ui.clone(),
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
    );

    // Handlers for opening the folder selection dialog and initiating scanning.
    // These handlers are connected to the "Add Music" buttons for both albums and artists,
    // allowing users to add music folders from either view.
    let albums_add_folder_handler = create_add_folder_dialog_handler(
        widgets.window.clone(),
        widgets.scanning_label_albums.clone(),
        db_pool.clone(),
        sender.clone(),
        widgets.albums_stack_cell.clone(),
    );
    add_music_button_albums.connect_clicked(move |_| {
        albums_add_folder_handler();
    });
    let artists_add_folder_handler = create_add_folder_dialog_handler(
        widgets.window.clone(),
        widgets.scanning_label_artists.clone(),
        db_pool.clone(),
        sender.clone(),
        widgets.artists_stack_cell.clone(),
    );
    add_music_button_artists.connect_clicked(move |_| {
        artists_add_folder_handler();
    });

    // Rebuild and populate initial grids for albums and artists.
    // These functions create the `FlowBox` grids and their containing `Stack`s,
    // and then populate them with initial data or empty states. This ensures the UI
    // is ready to display content as soon as the application launches.
    rebuild_albums_grid_for_window(
        &widgets.stack,
        &widgets.scanning_label_albums,
        &screen_info_cloned,
        &widgets.albums_grid_cell,
        &widgets.albums_stack_cell,
        &add_music_button_albums,
        widgets.album_count_label.clone(),
        view_mode_cloned.clone(),
    );
    rebuild_artist_grid_for_window(
        &widgets.stack,
        &widgets.scanning_label_artists,
        &widgets.artist_grid_cell,
        &widgets.artists_stack_cell,
        &add_music_button_artists,
        widgets.artist_count_label.clone(),
        view_mode_cloned.clone(),
    );

    // Setup live monitor refresh to adapt UI to screen size changes.
    // This periodically checks screen dimensions and recalculates cover/tile sizes if needed,
    // ensuring the UI remains aesthetically pleasing and functional across different display
    // configurations without requiring an application restart.
    setup_live_monitor_refresh(
        refresh_service.clone(),
        screen_info_cloned.clone(),
        is_settings_open_cloned.clone(),
    );

    // Start the library watcher for real-time file system changes.
    // This background task monitors the music library folders for new, modified, or deleted files,
    // automatically triggering UI updates to keep the library synchronized with the file system.
    start_watching_library(db_pool.clone(), sender.clone());

    // Initial connection for album navigation (clicking on an album tile).
    // This handler enables users to click on an album tile to navigate to its detailed page.
    // This handler will be re-connected whenever the albums grid is rebuilt to ensure
    // all dynamically created album tiles are interactive.
    if let Some(albums_grid) = widgets.albums_grid_cell.borrow().as_ref() {
        let show_dr_badges_clone = show_dr_badges_cloned.clone();
        let player_bar_clone = widgets.player_bar.clone();
        connect_album_navigation(
            albums_grid,
            &widgets.stack,
            db_pool.clone(),
            &widgets.left_btn_stack,
            &widgets.right_btn_box,
            nav_history_cloned.clone(),
            sender.clone(),
            move |stack_weak,
                  db_pool,
                  album_id,
                  left_btn_stack_weak,
                  right_btn_box_weak,
                  sender| {
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

    // Connect tab navigation (Albums/Artists buttons in the header).
    // This handles switching between the main Albums and Artists views.
    connect_tab_navigation(
        &widgets.albums_btn,
        &widgets.artists_btn,
        &widgets.stack,
        &widgets.sort_button,
        &widgets.left_btn_stack,
        &widgets.right_btn_box,
        last_tab_cloned.clone(),
        nav_history_cloned.clone(),
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
        refresh_library_ui.clone(),
        None::<fn()>,
    );

    // Connect sorting logic for tab toggles and sort icon updates.
    // Ensures that when tabs are switched or sort preferences change, the UI reflects
    // the correct sort icon and triggers a refresh of the displayed content.
    connect_tab_sort_refresh(
        &widgets.albums_btn,
        &widgets.artists_btn,
        refresh_library_ui.clone(),
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
        Rc::new(widgets.stack.clone()),
    );
    connect_sort_icon_update_on_tab_switch(
        &widgets.sort_button,
        &widgets.stack,
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
    );

    // Connect sort button logic to toggle sort order and refresh UI.
    // This allows users to change the sorting direction (ascending/descending) for the
    // currently active library view and updates the UI to reflect the new order.
    connect_sort_button(
        &widgets.sort_button,
        &widgets.stack,
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
        refresh_library_ui.clone(),
    );

    // Spawn scanning label refresh task to hide labels after scan completion.
    // This listens for signals from the library scanner (e.g., when a scan finishes)
    // and hides the "Scanning..." labels, providing clear visual feedback to the user.
    let receiver = Rc::new(RefCell::new(receiver)); // Wrap receiver in Rc<RefCell> for shared mutable access
    spawn_scanning_label_refresh_task(
        receiver,
        Rc::new(widgets.scanning_label_albums.clone()),
        Rc::new(widgets.scanning_label_artists.clone()),
        widgets.stack.clone(),
        refresh_library_ui.clone(),
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
    );

    // Connect live search functionality to the search entry.
    // As the user types, this triggers real-time searches in the database and dynamically
    // updates the displayed album and artist grids with matching results.
    connect_live_search(
        &widgets.search_bar.search_bar.entry,
        widgets.albums_grid_cell.borrow().as_ref().unwrap(),
        widgets.albums_stack_cell.borrow().as_ref().unwrap(),
        widgets.artist_grid_cell.borrow().as_ref().unwrap(),
        widgets.artists_stack_cell.borrow().as_ref().unwrap(),
        db_pool.clone(),
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
        refresh_library_ui.clone(),
        Rc::new(widgets.stack.clone()),
        Rc::new(widgets.left_btn_stack.clone()),
        Rc::new(widgets.right_btn_box.clone()),
        nav_history_cloned.clone(),
        sender.clone(),
        show_dr_badges_cloned.clone(),
        use_original_year_cloned.clone(),
        widgets.player_bar.clone(),
    );

    // Set up search bar UI logic (e.g., showing/hiding, focus management).
    // This integrates the search bar's behavior into the main window's UI flow.
    widgets
        .search_bar
        .search_bar
        .setup_logic(&widgets.window, vbox_inner);

    // Setup global keyboard shortcuts.
    // Configures keyboard shortcuts for common actions, such as using the Escape key
    // for back navigation or to close the search bar, enhancing user accessibility.
    setup_keyboard_shortcuts(
        &widgets.window,
        &widgets.search_bar.search_bar,
        &refresh_library_ui,
        &sort_ascending_cloned,
        &sort_ascending_artists_cloned,
        &widgets.stack,
        &widgets.left_btn_stack,
        &widgets.right_btn_box,
        &last_tab_cloned,
        &nav_history_cloned,
    );

    // Connect "Add Folder" dialog to its button.
    // This allows users to add new music folders to their library via a file chooser dialog.
    let add_folder_handler = create_add_folder_dialog_handler(
        widgets.window.clone(),
        widgets.scanning_label_albums.clone(),
        db_pool.clone(),
        sender.clone(),
        widgets.albums_stack_cell.clone(),
    );
    widgets.add_button.connect_clicked(move |_| {
        add_folder_handler();
    });

    // Connect settings dialog to its button.
    // This opens the application's settings dialog, allowing users to configure preferences
    // such as sorting orders and other application behaviors.
    connect_settings_dialog(
        &widgets.settings_button,
        widgets.window.clone(),
        sort_orders_cloned.clone(),
        refresh_library_ui.clone(),
        sort_ascending_cloned.clone(),
        sort_ascending_artists_cloned.clone(),
        db_pool.clone(),
        is_settings_open_cloned.clone(),
        show_dr_badges_cloned.clone(),
        use_original_year_cloned.clone(),
        view_mode_cloned.clone(),
    );
}

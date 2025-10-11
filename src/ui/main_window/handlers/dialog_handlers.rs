use std::{rc::Rc, sync::Arc};

use gtk4::{Box, Button};
use libadwaita::prelude::ButtonExt;
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::dialogs::{connect_settings_dialog, create_add_folder_dialog_handler},
    main_window::{state::WindowSharedState, widgets::WindowWidgets},
};

/// Connects the "Add Music" button handlers for both albums and artists views.
///
/// This function sets up click handlers for the "Add Music" buttons in both the albums
/// and artists views. When clicked, these handlers will:
/// - Open a folder selection dialog
/// - Initiate a music library scan of the selected folder
/// - Update the UI to show scanning progress
///
/// # Parameters
/// - `widgets`: Reference to the main window widgets containing UI elements
/// - `_shared_state`: Shared application state (currently unused in this function)
/// - `db_pool`: Database connection pool for storing scanned music data
/// - `sender`: Channel sender for communicating scan events to other parts of the application
/// - `_vbox_inner`: Reference to the inner container box (currently unused)
/// - `add_music_button_albums`: The "Add Music" button in the albums view
/// - `add_music_button_artists`: The "Add Music" button in the artists view
pub fn connect_add_folder_handlers(
    widgets: &WindowWidgets,
    _shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    _vbox_inner: &Box,
    add_music_button_albums: &Button,
    add_music_button_artists: &Button,
) {
    // Create the folder dialog handler for the albums view
    // This closure captures the necessary widgets and state for handling folder selection
    let albums_add_folder_handler = create_add_folder_dialog_handler(
        widgets.window.clone(),
        widgets.scanning_label_albums.clone(),
        db_pool.clone(),
        sender.clone(),
        widgets.albums_stack_cell.clone(),
    );

    // Connect the click handler for the albums "Add Music" button
    // When clicked, it will execute the folder selection and scanning process
    add_music_button_albums.connect_clicked(move |_| {
        albums_add_folder_handler();
    });

    // Create the folder dialog handler for the artists view
    // Similar to albums handler but with artists-specific widgets
    let artists_add_folder_handler = create_add_folder_dialog_handler(
        widgets.window.clone(),
        widgets.scanning_label_artists.clone(),
        db_pool.clone(),
        sender.clone(),
        widgets.artists_stack_cell.clone(),
    );

    // Connect the click handler for the artists "Add Music" button
    // When clicked, it will execute the folder selection and scanning process
    add_music_button_artists.connect_clicked(move |_| {
        artists_add_folder_handler();
    });
}

/// Connects the settings dialog handler to the settings button.
///
/// This function sets up the click handler for the settings button which opens the
/// application settings dialog. The settings dialog allows users to configure:
/// - Library sorting preferences
/// - Display options like DR badges
/// - Year display preferences
/// - Album metadata visibility
/// - Other application behaviors
///
/// # Parameters
/// - `widgets`: Reference to the main window widgets containing UI elements
/// - `shared_state`: Shared application state containing current settings values
/// - `db_pool`: Database connection pool for persisting settings changes
/// - `sender`: Channel sender for communicating settings changes to other parts of the application
/// - `refresh_library_ui`: Callback function to refresh the library UI when settings change
pub fn connect_settings_dialog_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    // Delegate to the settings dialog connection function in the components module
    // This function handles all the complex logic of creating and managing the settings dialog
    connect_settings_dialog(
        &widgets.settings_button,
        widgets.window.clone(),
        shared_state.sort_orders.clone(),
        refresh_library_ui.clone(),
        shared_state.sort_ascending.clone(),
        shared_state.sort_ascending_artists.clone(),
        db_pool.clone(),
        shared_state.is_settings_open.clone(),
        shared_state.show_dr_badges.clone(),
        shared_state.use_original_year.clone(),
        shared_state.show_album_metadata.clone(),
        Some(sender.clone()),
        widgets.scanning_label_albums.clone(),
        widgets.scanning_label_artists.clone(),
        widgets.albums_stack_cell.clone(),
        widgets.artists_stack_cell.clone(),
    );
}

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, idle_add_local_once};
use gtk4::{Label, ListBox, Stack, prelude::WidgetExt};
use libadwaita::PreferencesGroup;
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::{cleanup::remove_folder_and_albums, query::fetch_all_folders},
    ui::settings::folder::{
        FolderSettingsPage,
        event_handlers::connect_remove_folder_handler,
        ui_components::{create_empty_state_row, create_folder_row},
    },
};

/// Refreshes the display of library folders by fetching them from the database
/// and updating the `ListBox`.
///
/// This method is asynchronous as it performs database queries. It handles
/// the UI update on the main thread using `idle_add_local_once`.
///
/// # Arguments
///
/// * `folder_settings_page` - The folder settings page instance
pub async fn refresh_display(folder_settings_page: &FolderSettingsPage) {
    let folders = fetch_all_folders(&folder_settings_page.db_pool)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Failed to fetch folders: {}", e);
            vec![]
        });

    // Clone references for the `idle_add_local_once` closure.
    let folders_group_c = folder_settings_page.folders_group.clone();
    let list_box_c = folder_settings_page.list_box.clone();
    let db_pool_c = folder_settings_page.db_pool.clone();
    let refresh_library_ui_c = folder_settings_page.refresh_library_ui.clone();
    let sort_ascending_c = folder_settings_page.sort_ascending.clone();
    let sort_ascending_artists_c = folder_settings_page.sort_ascending_artists.clone();
    let main_context_c = folder_settings_page.main_context.clone();
    let sender_c = folder_settings_page.sender.clone();

    // Clone the fetched folders for the closure
    let folders_c = folders.clone();
    let scanning_label_albums = folder_settings_page.scanning_label_albums.clone();
    let scanning_label_artists = folder_settings_page.scanning_label_artists.clone();
    let albums_stack_cell = folder_settings_page.albums_stack_cell.clone();
    let artists_stack_cell = folder_settings_page.artists_stack_cell.clone();
    idle_add_local_once(move || {
        // Clear all existing children from the ListBox before repopulating.
        while let Some(child) = list_box_c.first_child() {
            list_box_c.remove(&child);
        }

        // Sort folders alphabetically by path for consistent display.
        let mut sorted_folders = folders_c;
        sorted_folders.sort_by(|a, b| {
            a.path
                .to_str()
                .unwrap_or_default()
                .to_lowercase()
                .cmp(&b.path.to_str().unwrap_or_default().to_lowercase())
        });
        if sorted_folders.is_empty() {
            // Display a message if no folders are added.
            let empty_row = create_empty_state_row();
            list_box_c.append(&empty_row);
        } else {
            // Populate the ListBox with an ActionRow for each folder.
            for folder in &sorted_folders {
                let folder_path = folder.path.to_str().unwrap_or_default().trim();
                if folder_path.is_empty() {
                    continue;
                }

                let (row, remove_btn) = create_folder_row(folder_path);
                let folder_id = folder.id;

                // Clone the necessary fields for the closure.
                let folders_group = folders_group_c.clone();
                let list_box = list_box_c.clone();
                let db_pool = db_pool_c.clone();
                let refresh_library_ui = refresh_library_ui_c.clone();
                let sort_ascending = sort_ascending_c.clone();
                let sort_ascending_artists = sort_ascending_artists_c.clone();
                let main_context = main_context_c.clone();
                let sender = sender_c.clone();
                let scanning_label_albums = scanning_label_albums.clone();
                let scanning_label_artists = scanning_label_artists.clone();
                let albums_stack_cell = albums_stack_cell.clone();
                let artists_stack_cell = artists_stack_cell.clone();

                // Connect the remove folder handler
                let folder_settings_page = FolderSettingsPage {
                    folders_group,
                    list_box,
                    db_pool,
                    refresh_library_ui,
                    sort_ascending,
                    sort_ascending_artists,
                    main_context,
                    sender,
                    scanning_label_albums,
                    scanning_label_artists,
                    albums_stack_cell,
                    artists_stack_cell,
                };

                connect_remove_folder_handler(
                    &remove_btn,
                    Rc::new(folder_settings_page),
                    folder_id,
                );

                // Add the folder row to the ListBox
                list_box_c.append(&row);
            }
        }

        // Request re-allocation and re-drawing of the ListBox.
        list_box_c.queue_allocate();
        list_box_c.queue_draw();
    });
}

/// Handles the removal of a folder from the database and UI.
///
/// # Arguments
///
/// * `folders_group` - The preferences group
/// * `list_box` - The list box containing folders
/// * `db_pool` - The database connection pool
/// * `refresh_library_ui` - Callback to refresh the main library UI
/// * `sort_ascending` - Shared state for album sort direction
/// * `sort_ascending_artists` - Shared state for artist sort direction
/// * `main_context` - The GLib main context for spawning UI tasks
/// * `sender` - Optional sender to notify UI refresh after scanning
/// * `scanning_label_albums` - The scanning label for albums
/// * `scanning_label_artists` - The scanning label for artists
/// * `albums_stack_cell` - The albums stack cell
/// * `artists_stack_cell` - The artists stack cell
/// * `folder_id` - The ID of the folder to remove
#[allow(clippy::too_many_arguments)]
pub fn handle_folder_removal(
    folders_group: Rc<PreferencesGroup>,
    list_box: Rc<ListBox>,
    db_pool: Arc<SqlitePool>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    main_context: Rc<MainContext>,
    sender: Option<UnboundedSender<()>>,
    scanning_label_albums: Rc<Label>,
    scanning_label_artists: Rc<Label>,
    albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    artists_stack_cell: Rc<RefCell<Option<Stack>>>,
    folder_id: i64,
) {
    // Spawn an asynchronous task on the main context.
    let folders_group = folders_group.clone();
    let list_box = list_box.clone();
    let db_pool = db_pool.clone();
    let refresh_library_ui = refresh_library_ui.clone();
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    let main_context = main_context.clone();
    let sender = sender.clone();
    let scanning_label_albums = scanning_label_albums.clone();
    let scanning_label_artists = scanning_label_artists.clone();
    let albums_stack_cell = albums_stack_cell.clone();
    let artists_stack_cell = artists_stack_cell.clone();
    main_context.clone().spawn_local(async move {
        // Perform database deletion.
        let _ = remove_folder_and_albums(&db_pool, folder_id).await;

        // Refresh main library UI.
        refresh_library_ui(sort_ascending.get(), sort_ascending_artists.get());

        // Create a new FolderSettingsPage instance for refreshing the display.
        let folder_settings_page = FolderSettingsPage {
            folders_group,
            list_box,
            db_pool,
            refresh_library_ui,
            sort_ascending,
            sort_ascending_artists,
            main_context,
            sender,
            scanning_label_albums,
            scanning_label_artists,
            albums_stack_cell,
            artists_stack_cell,
        };

        // Refresh the folder display in the settings dialog.
        refresh_display(&folder_settings_page).await;
    });
}

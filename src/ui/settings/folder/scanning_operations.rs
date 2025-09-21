use std::{path::PathBuf, rc::Rc, thread::spawn, time::Duration};

use gtk4::glib::timeout_future;
use libadwaita::prelude::WidgetExt;
use tokio::runtime::Runtime;

use crate::{
    data::{db::crud::insert_or_get_folder, scanner::scan_folder},
    ui::settings::folder::data_operations::refresh_display,
};

use super::core::FolderSettingsPage;

/// Handles the addition of a new folder, including scanning and UI updates.
///
/// # Arguments
///
/// * `folder_settings_page` - The folder settings page instance
/// * `path` - The path of the folder to add
pub fn handle_folder_addition(folder_settings_page: &FolderSettingsPage, path: PathBuf) {
    // Show scanning feedback before starting the scan
    // Make the scanning labels visible
    folder_settings_page.scanning_label_albums.set_visible(true);
    folder_settings_page
        .scanning_label_artists
        .set_visible(true);

    // Set the appropriate stacks to scanning state if they exist
    if let Some(stack) = folder_settings_page.albums_stack_cell.borrow().as_ref() {
        stack.set_visible_child_name("scanning_state");
    }
    if let Some(stack) = folder_settings_page.artists_stack_cell.borrow().as_ref() {
        stack.set_visible_child_name("scanning_state");
    }

    // Extract the necessary fields before spawning the thread
    let db_pool = folder_settings_page.db_pool.clone();
    let sender = folder_settings_page.sender.clone();
    let path_clone = path.clone();

    // Spawn a new thread for blocking I/O and async operations
    spawn(move || {
        // Create a new Tokio runtime for this thread
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Insert folder into DB or get existing ID.
            let folder_id = match insert_or_get_folder(&db_pool, &path_clone).await {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Error inserting or getting folder: {:?}", e);
                    return;
                }
            };

            // Scan the folder for music files.
            if let Err(e) = scan_folder(&db_pool, &path_clone, folder_id).await {
                eprintln!("Error scanning folder: {:?}", e);
            }

            // Notify the main thread that scanning is complete
            if let Some(sender) = sender
                && let Err(e) = sender.send(())
            {
                eprintln!("Error sending refresh signal: {:?}", e);
            }
        });
    });

    // Clone self_clone for use in the async block
    let self_clone_for_async = Rc::new(folder_settings_page.clone_for_closure());

    // Spawn a local async task to refresh the display after a short delay
    // This gives time for the database operation to complete
    folder_settings_page.main_context.spawn_local(async move {
        // Small delay to ensure database operation completes
        timeout_future(Duration::from_millis(500)).await;

        // Refresh the folder display in the settings dialog.
        refresh_display(&self_clone_for_async).await;
    });
}

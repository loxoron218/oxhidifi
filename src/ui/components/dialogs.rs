use std::{
    boxed::Box,
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::Arc,
    thread::spawn,
};

use gtk4::{Button, FileDialog, Label, Stack, Widget, Window, glib::MainContext};
use libadwaita::{
    AlertDialog,
    ResponseAppearance::Destructive,
    prelude::{AdwDialogExt, AlertDialogExt, ButtonExt, FileExt, IsA, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::{
    data::{db::crud::insert_or_get_folder, scanner::scan_folder},
    ui::{
        components::view_controls::sorting_controls::types::SortOrder,
        settings::settings_dialog::show_settings_dialog,
    },
};

/// Handles the logic for displaying a folder chooser dialog and initiating a library scan.
///
/// This function is typically connected to a button click event. When the user
/// selects a folder and confirms, it triggers an asynchronous process to:
/// 1. Update the UI to indicate scanning is in progress.
/// 2. Insert or retrieve the folder's ID in the database.
/// 3. Scan the selected folder for music files.
/// 4. Notify the main thread to refresh the UI after the scan is complete.
///
/// # Arguments
/// * `parent_window` - The parent GTK window for the dialog, ensuring it's modal.
/// * `scanning_label` - A `Label` widget used to display scanning feedback.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations.
/// * `sender` - An `UnboundedSender<()>` to send a signal to the main thread upon scan completion.
/// * `albums_inner_stack` - An `Rc<RefCell<Option<Stack>>>` to control the visibility of UI elements
///   (e.g., showing a "scanning" state or the album grid).
///
/// # Returns
/// A `Box<dyn Fn() + 'static>` closure that can be connected to a GTK button's `clicked` signal.
pub fn create_add_folder_dialog_handler<T: IsA<Window> + Clone + 'static>(
    parent_window: T,
    scanning_label: Label,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    albums_inner_stack: Rc<RefCell<Option<Stack>>>,
) -> Box<dyn Fn()> {
    // Clone necessary variables for the outer closure to move them into the returned closure
    let scanning_label_cloned = scanning_label;
    let db_pool_cloned = db_pool;
    let sender_cloned = sender;
    let parent_window_cloned = parent_window;
    let albums_inner_stack_cloned = albums_inner_stack;
    Box::new(move || {
        let dialog = FileDialog::new();
        dialog.set_title("Open Folder");

        // Clone variables again for the async closure, which will be moved.
        let scanning_label_for_response = scanning_label_cloned.clone();
        let db_pool_for_response = db_pool_cloned.clone();
        let sender_for_response = sender_cloned.clone();
        let albums_inner_stack_for_response = albums_inner_stack_cloned.clone();

        // Spawn the async file dialog operation
        let parent_window_for_async = parent_window_cloned.clone();
        MainContext::default().spawn_local(async move {
            match dialog
                .select_folder_future(Some(&parent_window_for_async))
                .await
            {
                Ok(folder) => {
                    if let Some(folder_path) = folder.path() {
                        let folder_path_string = folder_path.to_string_lossy().to_string();

                        // This action always happens, so we perform it first.
                        scanning_label_for_response.set_visible(true);

                        // Then, we conditionally perform the action that depends on the stack.
                        if let Some(stack) = albums_inner_stack_for_response.borrow().as_ref() {
                            stack.set_visible_child_name("scanning_state");
                        }

                        // Clone for the spawned thread, which needs its own ownership
                        let db_pool_for_spawn = db_pool_for_response.clone();
                        let folder_path_string_for_spawn = folder_path_string.clone();
                        let sender_for_spawn = sender_for_response.clone();

                        // Spawn a new thread for blocking I/O and async operations
                        spawn(move || {
                            // Create a new Tokio runtime for this thread
                            let rt = Runtime::new().unwrap();
                            rt.block_on(async {
                                // Insert folder into DB or get existing ID
                                let folder_id = match insert_or_get_folder(
                                    &db_pool_for_spawn,
                                    Path::new(&folder_path_string_for_spawn),
                                )
                                .await
                                {
                                    Ok(id) => id,
                                    Err(e) => {
                                        eprintln!("Error inserting or getting folder: {:?}", e);

                                        // Exit on error
                                        return;
                                    }
                                };

                                // Scan the folder for music files
                                if let Err(e) = scan_folder(
                                    &db_pool_for_spawn,
                                    Path::new(&folder_path_string_for_spawn),
                                    folder_id,
                                )
                                .await
                                {
                                    eprintln!("Error scanning folder: {:?}", e);
                                }

                                // Notify the main thread to refresh the UI
                                if let Err(e) = sender_for_spawn.send(()) {
                                    eprintln!("Error sending refresh signal: {:?}", e);
                                }
                            });
                        });
                    }
                }
                Err(_) => {
                    // User cancelled the dialog - do nothing
                }
            }
        });
    })
}

/// Connects the settings button to open the settings dialog asynchronously.
///
/// This function sets up a `clicked` signal handler for the provided `settings_button`.
/// When the button is clicked, it spawns a new local asynchronous task on the GLib
/// main context to display the settings dialog. This ensures the UI remains
/// responsive while the settings dialog is being prepared and shown.
///
/// # Arguments
/// * `settings_button` - A reference to the `gtk4::Button` that triggers the settings dialog.
/// * `parent_window` - The parent `gtk4::Window` for the settings dialog.
/// * `sort_orders` - An `Rc<RefCell<Vec<SortOrder>>>` containing the current sort order settings.
/// * `refresh_library_ui` - A closure (`Rc<dyn Fn(bool, bool)>`) to refresh the library UI.
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the sort direction for artists.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations within the settings dialog.
/// * `is_settings_open` - An `Rc<Cell<bool>>` flag to track if the settings dialog is currently open.
/// * `show_dr_badges_setting` - An `Rc<Cell<bool>>` flag for showing DR badges.
/// * `use_original_year_setting` - An `Rc<Cell<bool>>` flag for using original release year.
/// * `sender` - Optional sender to notify UI refresh after scanning.
pub fn connect_settings_dialog<P: IsA<Window> + IsA<Widget> + Clone + 'static>(
    settings_button: &Button,
    parent_window: P,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    db_pool: Arc<SqlitePool>,
    is_settings_open: Rc<Cell<bool>>,
    show_dr_badges_setting: Rc<Cell<bool>>,
    use_original_year_setting: Rc<Cell<bool>>,
    show_album_metadata_setting: Rc<Cell<bool>>,
    sender: Option<UnboundedSender<()>>,
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    artists_stack_cell: Rc<RefCell<Option<Stack>>>,
) {
    // Clone all necessary `Rc` and `Arc` variables once for the `connect_clicked` closure.
    // These clones will be moved into the outer closure.
    let window_clone = parent_window;
    let sort_orders_cloned = sort_orders;
    let refresh_library_ui_cloned = refresh_library_ui;
    let sort_ascending_cloned = sort_ascending;
    let sort_ascending_artists_cloned = sort_ascending_artists;
    let db_pool_cloned = db_pool;
    let is_settings_open_cloned = is_settings_open;
    let show_dr_badges_setting_cloned = show_dr_badges_setting;
    let show_album_metadata_setting_cloned = show_album_metadata_setting;
    let sender_clone = sender.clone();
    settings_button.connect_clicked(move |_| {
        // Clone variables again for the `spawn_local` async block.
        // These clones will be moved into the inner async closure.
        let db_pool_for_async = db_pool_cloned.clone();
        let refresh_library_ui_for_async = refresh_library_ui_cloned.clone();
        let sort_ascending_for_async = sort_ascending_cloned.clone();
        let sort_ascending_artists_for_async = sort_ascending_artists_cloned.clone();
        let window_for_async = window_clone.clone();
        let sort_orders_for_async = sort_orders_cloned.clone();
        let is_settings_open_for_async = is_settings_open_cloned.clone();
        let show_dr_badges_setting_for_async = show_dr_badges_setting_cloned.clone();
        let use_original_year_setting_for_async = use_original_year_setting.clone();
        let show_album_metadata_setting_for_async = show_album_metadata_setting_cloned.clone();
        let sender_for_async = sender_clone.clone();
        let scanning_label_albums_clone = scanning_label_albums.clone();
        let scanning_label_artists_clone = scanning_label_artists.clone();
        let albums_stack_cell_clone = albums_stack_cell.clone();
        let artists_stack_cell_clone = artists_stack_cell.clone();
        MainContext::default().spawn_local(async move {
            show_settings_dialog(
                &window_for_async,
                sort_orders_for_async,
                refresh_library_ui_for_async,
                sort_ascending_for_async,
                sort_ascending_artists_for_async,
                db_pool_for_async,
                is_settings_open_for_async,
                show_dr_badges_setting_for_async,
                use_original_year_setting_for_async,
                show_album_metadata_setting_for_async,
                sender_for_async,
                scanning_label_albums_clone,
                scanning_label_artists_clone,
                albums_stack_cell_clone,
                artists_stack_cell_clone,
            );
        });
    });
}

/// Displays a confirmation dialog before removing a folder from the library.
///
/// This dialog provides a warning message about the consequences of removing a folder,
/// such as the deletion of custom metadata. If the user confirms the action by clicking
/// "OK", the provided `on_confirm` closure is executed. The "OK" button is styled
/// as a "destructive-action" to visually indicate its impact.
///
/// # Arguments
/// * `parent` - The parent `gtk4::Window` for the dialog, ensuring it's modal and transient.
/// * `on_confirm` - A closure that will be executed if the user confirms the removal.
///   It must be `FnOnce` as it's typically called only once.
pub fn show_remove_folder_confirmation_dialog<F: FnOnce() + 'static>(
    parent: &impl IsA<Window>,
    on_confirm: F,
) {
    let on_confirm_rc = Rc::new(RefCell::new(Some(on_confirm)));
    let dialog = AlertDialog::new(
        Some("Remove Folder?"),
        Some(
            "Removing this folder will delete all songs, albums, artists, and custom metadata associated with this folder from the database. This action cannot be undone.",
        ),
    );

    // Configure responses
    dialog.add_response("cancel", "_Cancel");
    dialog.add_response("ok", "_OK");

    // Set the OK response as destructive
    dialog.set_response_appearance("ok", Destructive);

    // Set default and close responses
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    // Connect to the response signal
    dialog.connect_response(None, move |dialog, response| {
        if response == "ok" {
            // Execute the on_confirm closure if it exists (i.e., hasn't been taken yet)
            if let Some(f) = on_confirm_rc.borrow_mut().take() {
                f();
            }
        }

        // The dialog is automatically closed after the response
        let _ = dialog;
    });

    // Present the dialog
    dialog.present(Some(parent.as_ref()));
}

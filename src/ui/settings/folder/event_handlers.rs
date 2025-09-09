use std::rc::Rc;

use gtk4::{
    Button,
    FileChooserAction::SelectFolder,
    FileChooserDialog,
    ResponseType::{Accept, Cancel},
    Window,
};
use libadwaita::prelude::{
    ButtonExt, CastNone, DialogExt, FileChooserExt, FileExt, GtkWindowExt, StaticType, WidgetExt,
};

use crate::ui::{
    components::dialogs::show_remove_folder_confirmation_dialog,
    settings::folder::{
        FolderSettingsPage, data_operations::handle_folder_removal,
        scanning_operations::handle_folder_addition,
    },
};

/// Connects the "Add Folder" button handler to the provided button.
///
/// # Arguments
///
/// * `add_folder_btn` - The "Add Folder" button to connect
/// * `folder_settings_page` - The folder settings page instance
pub fn connect_add_folder_handler(
    add_folder_btn: &Button,
    folder_settings_page: Rc<FolderSettingsPage>,
) {
    // Clone the page instance for use in the click handler closure
    let self_clone = folder_settings_page.clone();

    // Connect the click handler for the "Add Folder" button
    add_folder_btn.connect_clicked(move |_| {
        handle_add_folder_clicked(&self_clone);
    });
}

/// Handles the click event for the "Add Folder" button.
///
/// This function creates and displays a `FileChooserDialog` to allow the user
/// to select a folder. Upon confirmation, it triggers the process of adding
/// the folder to the database, scanning it, and refreshing the UI.
///
/// # Arguments
///
/// * `folder_settings_page` - The folder settings page instance
fn handle_add_folder_clicked(folder_settings_page: &FolderSettingsPage) {
    let binding = folder_settings_page
        .folders_group
        .ancestor(Window::static_type());
    let parent_window = binding.and_downcast_ref::<Window>();
    let dialog = FileChooserDialog::new(
        Some("Add Folder to Library"),
        parent_window,
        SelectFolder,
        &[("Cancel", Cancel), ("Add", Accept)],
    );
    dialog.set_modal(true);

    // Clone the necessary state for the response handler closure.
    let self_clone = Rc::new(folder_settings_page.clone_for_closure());
    dialog.connect_response(move |dialog, response| {
        if response == Accept {
            if let Some(folder) = dialog.file() {
                if let Some(path) = folder.path() {
                    // Handle the folder addition (delegated to scanning_operations module)
                    handle_folder_addition(&self_clone, path);
                }
            }
        }
        dialog.close();
    });
    dialog.show();
}

/// Connects the remove folder handler to the provided button.
///
/// # Arguments
///
/// * `remove_btn` - The remove button to connect
/// * `folder_settings_page` - The folder settings page instance
/// * `folder_id` - The ID of the folder to remove
pub fn connect_remove_folder_handler(
    remove_btn: &Button,
    folder_settings_page: Rc<FolderSettingsPage>,
    folder_id: i64,
) {
    // Clone the necessary fields for the closure.
    let folders_group = folder_settings_page.folders_group.clone();
    let list_box = folder_settings_page.list_box.clone();
    let db_pool = folder_settings_page.db_pool.clone();
    let refresh_library_ui = folder_settings_page.refresh_library_ui.clone();
    let sort_ascending = folder_settings_page.sort_ascending.clone();
    let sort_ascending_artists = folder_settings_page.sort_ascending_artists.clone();
    let main_context = folder_settings_page.main_context.clone();
    let sender = folder_settings_page.sender.clone();
    let scanning_label_albums = folder_settings_page.scanning_label_albums.clone();
    let scanning_label_artists = folder_settings_page.scanning_label_artists.clone();
    let albums_stack_cell = folder_settings_page.albums_stack_cell.clone();
    let artists_stack_cell = folder_settings_page.artists_stack_cell.clone();
    remove_btn.connect_clicked(move |btn| {
        let binding = btn.ancestor(Window::static_type());
        let parent_widget = binding
            .and_downcast_ref::<Window>()
            .expect("Button should be within a window hierarchy.");

        // Clone the necessary fields for the `on_confirm` closure of the dialog.
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
        show_remove_folder_confirmation_dialog(parent_widget, move || {
            // Handle the folder removal (delegated to data_operations module)
            handle_folder_removal(
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
                folder_id,
            );
        });
    });
}

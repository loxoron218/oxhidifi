use gtk4::{Align::Center, Button, ListBox, SelectionMode::None};
use libadwaita::{
    ActionRow, PreferencesGroup,
    prelude::{ActionRowExt, PreferencesGroupExt, WidgetExt},
};

/// Creates the main UI components for the folder settings page.
///
/// This function creates and configures the preferences group, list box, and add folder button.
///
/// # Returns
///
/// A tuple containing:
/// * The configured `PreferencesGroup`
/// * The configured `ListBox`
/// * The configured "Add Folder" `Button`
pub fn create_folder_ui() -> (PreferencesGroup, ListBox, Button) {
    // Create the main preferences group for library folders with a title and description
    let folders_group = PreferencesGroup::builder()
        .title("Library Folders")
        .description("Add or remove folders to be scanned for music.")
        .build();

    // Create the "Add Folder" button with appropriate styling and icon
    let add_folder_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .valign(Center)
        .css_classes(["flat"])
        .build();

    // Add the "Add Folder" button to the header of the preferences group
    folders_group.set_header_suffix(Some(&add_folder_btn));

    // Create the list box that will display the library folders
    let list_box = ListBox::new();

    // Disable selection mode since we're only displaying folders with remove buttons
    list_box.set_selection_mode(None);

    (folders_group, list_box, add_folder_btn)
}

/// Creates an empty state row for when no folders are added.
///
/// # Returns
///
/// An `ActionRow` with appropriate styling for the empty state.
pub fn create_empty_state_row() -> ActionRow {
    let empty_row = ActionRow::builder()
        .title("No folders added.")
        .activatable(false)
        .selectable(false)
        .build();

    // Apply styling for dimmed text
    empty_row.add_css_class("dim-label");
    empty_row
}

/// Creates a folder row for displaying a folder in the list.
///
/// # Arguments
///
/// * `folder_path` - The path of the folder to display
///
/// # Returns
///
/// A tuple containing:
/// * The configured `ActionRow` for the folder
/// * The configured "Remove" `Button` for the folder
pub fn create_folder_row(folder_path: &str) -> (ActionRow, Button) {
    let row = ActionRow::builder().title(folder_path).build();
    let remove_btn = Button::builder()
        .icon_name("window-close-symbolic")
        .valign(Center)
        .css_classes(["flat"])
        .build();

    // Add the remove button to the right of the row
    row.add_suffix(&remove_btn);
    (row, remove_btn)
}

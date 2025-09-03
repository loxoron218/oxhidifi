use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    thread::spawn,
};

use glib::{MainContext, idle_add_local_once};
use gtk4::{
    Align::Center,
    Button,
    FileChooserAction::SelectFolder,
    FileChooserDialog, Label, ListBox,
    ResponseType::{Accept, Cancel},
    SelectionMode::None,
    Stack, Window,
};
use libadwaita::{
    ActionRow, PreferencesGroup,
    prelude::{
        ActionRowExt, ButtonExt, CastNone, DialogExt, FileChooserExt, FileExt, GtkWindowExt,
        PreferencesGroupExt, StaticType, WidgetExt,
    },
};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::{
    data::{
        db::{
            cleanup::remove_folder_and_albums, crud::insert_or_get_folder, query::fetch_all_folders,
        },
        scanner::scan_folder,
    },
    ui::components::dialogs::show_remove_folder_confirmation_dialog,
};

/// Manages the UI and logic for the "Library Folders" section within the settings dialog.
///
/// This struct encapsulates the `PreferencesGroup` and `ListBox` responsible for
/// displaying and allowing the removal of library folders. It holds references to
/// shared application state necessary for interacting with the database and
/// refreshing the main library UI.
pub struct FolderSettingsPage {
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
}

impl FolderSettingsPage {
    /// Creates a new `FolderSettingsPage` instance, initializing its UI components
    /// and holding necessary shared state.
    ///
    /// # Arguments
    ///
    /// * `db_pool` - The database connection pool.
    /// * `refresh_library_ui` - Callback to refresh the main library UI.
    /// * `sort_ascending` - Shared state for album sort direction.
    /// * `sort_ascending_artists` - Shared state for artist sort direction.
    /// * `main_context` - The GLib main context for spawning UI tasks.
    /// * `sender` - Optional sender to notify UI refresh after scanning.
    /// * `scanning_label_albums` - The scanning label for albums.
    /// * `scanning_label_artists` - The scanning label for artists.
    /// * `albums_stack_cell` - The albums stack cell.
    /// * `artists_stack_cell` - The artists stack cell.
    ///
    /// # Returns
    ///
    /// A new `FolderSettingsPage` instance.
    pub fn new(
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
    ) -> Rc<Self> {
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

        // Add the list box to the preferences group
        folders_group.add(&list_box);

        // Create the FolderSettingsPage instance with all necessary components and shared state
        let folder_settings_page = Rc::new(Self {
            folders_group: Rc::new(folders_group),
            list_box: Rc::new(list_box),
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
        });

        // Clone the page instance for use in the click handler closure
        let self_clone = folder_settings_page.clone();

        // Connect the click handler for the "Add Folder" button
        add_folder_btn.connect_clicked(move |_| {
            self_clone.handle_add_folder_clicked();
        });
        folder_settings_page
    }

    /// Returns a reference to the `PreferencesGroup` for this page, allowing it to be added
    /// to a `PreferencesPage`.
    pub fn group(&self) -> &PreferencesGroup {
        &self.folders_group
    }

    /// Handles the click event for the "Add Folder" button.
    ///
    /// This function creates and displays a `FileChooserDialog` to allow the user
    /// to select a folder. Upon confirmation, it triggers the process of adding
    /// the folder to the database, scanning it, and refreshing the UI.
    fn handle_add_folder_clicked(&self) {
        let binding = self.folders_group.ancestor(Window::static_type());
        let parent_window = binding.and_downcast_ref::<Window>();
        let dialog = FileChooserDialog::new(
            Some("Add Folder to Library"),
            parent_window,
            SelectFolder,
            &[("Cancel", Cancel), ("Add", Accept)],
        );
        dialog.set_modal(true);

        // Clone the necessary state for the response handler closure.
        let self_clone = Rc::new(self.clone_for_closure());
        dialog.connect_response(move |dialog, response| {
            if response == Accept {
                if let Some(folder) = dialog.file() {
                    if let Some(path) = folder.path() {
                        // Show scanning feedback before starting the scan
                        // Make the scanning labels visible
                        self_clone.scanning_label_albums.set_visible(true);
                        self_clone.scanning_label_artists.set_visible(true);

                        // Set the appropriate stacks to scanning state if they exist
                        if let Some(stack) = self_clone.albums_stack_cell.borrow().as_ref() {
                            stack.set_visible_child_name("scanning_state");
                        }
                        if let Some(stack) = self_clone.artists_stack_cell.borrow().as_ref() {
                            stack.set_visible_child_name("scanning_state");
                        }

                        // Extract the necessary fields before spawning the thread
                        let db_pool = self_clone.db_pool.clone();
                        let sender = self_clone.sender.clone();
                        let path_clone = path.clone();

                        // Spawn a new thread for blocking I/O and async operations
                        spawn(move || {
                            // Create a new Tokio runtime for this thread
                            let rt = Runtime::new().unwrap();
                            rt.block_on(async {
                                // Insert folder into DB or get existing ID.
                                let folder_id =
                                    match insert_or_get_folder(&db_pool, &path_clone).await {
                                        Ok(id) => id,
                                        Err(e) => {
                                            eprintln!("Error inserting or getting folder: {:?}", e);
                                            return;
                                        }
                                    };

                                // Scan the folder for music files.
                                if let Err(e) = scan_folder(&db_pool, &path_clone, folder_id).await
                                {
                                    eprintln!("Error scanning folder: {:?}", e);
                                }

                                // Notify the main thread that scanning is complete
                                if let Some(sender) = sender {
                                    if let Err(e) = sender.send(()) {
                                        eprintln!("Error sending refresh signal: {:?}", e);
                                    }
                                }
                            });
                        });
                    }
                }
            }
            dialog.close();
        });
        dialog.show();
    }

    /// Creates a clone of `FolderSettingsPage` with its internal `Rc` and `Arc`
    /// fields also cloned. This is useful for passing the struct into closures
    /// without moving the original.
    fn clone_for_closure(&self) -> Self {
        Self {
            folders_group: self.folders_group.clone(),
            list_box: self.list_box.clone(),
            db_pool: self.db_pool.clone(),
            refresh_library_ui: self.refresh_library_ui.clone(),
            sort_ascending: self.sort_ascending.clone(),
            sort_ascending_artists: self.sort_ascending_artists.clone(),
            main_context: self.main_context.clone(),
            sender: self.sender.clone(),
            scanning_label_albums: self.scanning_label_albums.clone(),
            scanning_label_artists: self.scanning_label_artists.clone(),
            albums_stack_cell: self.albums_stack_cell.clone(),
            artists_stack_cell: self.artists_stack_cell.clone(),
        }
    }

    /// Refreshes the display of library folders by fetching them from the database
    /// and updating the `ListBox`.
    ///
    /// This method is asynchronous as it performs database queries. It handles
    /// the UI update on the main thread using `idle_add_local_once`.
    pub async fn refresh_display(&self) {
        let folders = fetch_all_folders(&self.db_pool).await.unwrap_or_else(|e| {
            eprintln!("Failed to fetch folders: {}", e);
            vec![]
        });

        // Clone references for the `idle_add_local_once` closure.
        let folders_group_c = self.folders_group.clone();
        let list_box_c = self.list_box.clone();
        let db_pool_c = self.db_pool.clone();
        let refresh_library_ui_c = self.refresh_library_ui.clone();
        let sort_ascending_c = self.sort_ascending.clone();
        let sort_ascending_artists_c = self.sort_ascending_artists.clone();
        let main_context_c = self.main_context.clone();
        let sender_c = self.sender.clone();

        // Clone the fetched folders for the closure
        let folders_c = folders.clone();
        let scanning_label_albums = self.scanning_label_albums.clone();
        let scanning_label_artists = self.scanning_label_artists.clone();
        let albums_stack_cell = self.albums_stack_cell.clone();
        let artists_stack_cell = self.artists_stack_cell.clone();
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
                let empty_row = ActionRow::builder()
                    .title("No folders added.")
                    .activatable(false)
                    .selectable(false)
                    .build();

                // Apply styling for dimmed text
                empty_row.add_css_class("dim-label");
                list_box_c.append(&empty_row);
            } else {
                // Populate the ListBox with an ActionRow for each folder.
                for folder in &sorted_folders {
                    let folder_path = folder.path.to_str().unwrap_or_default().trim();
                    if folder_path.is_empty() {
                        continue;
                    }
                    let row = ActionRow::builder().title(folder_path).build();
                    let remove_btn = Button::builder()
                        .icon_name("window-close-symbolic")
                        .valign(Center)
                        .css_classes(["flat"])
                        .build();
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
                                refresh_library_ui(
                                    sort_ascending.get(),
                                    sort_ascending_artists.get(),
                                );

                                // Create a new FolderSettingsPage instance for refreshing the display.
                                let folder_settings_page = Self {
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
                                folder_settings_page.refresh_display().await;
                            });
                        });
                    });

                    // Add the remove button to the right of the row
                    row.add_suffix(&remove_btn);

                    // Add the folder row to the ListBox
                    list_box_c.append(&row);
                }
            }

            // Request re-allocation and re-drawing of the ListBox.
            list_box_c.queue_allocate();
            list_box_c.queue_draw();
        });
    }
}

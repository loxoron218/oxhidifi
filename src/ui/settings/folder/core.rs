use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::MainContext;
use gtk4::{Label, ListBox, Stack};
use libadwaita::{PreferencesGroup, prelude::PreferencesGroupExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::settings::folder::{
    data_operations::refresh_display, event_handlers::connect_add_folder_handler,
};

use super::ui_components::create_folder_ui;

/// Manages the UI and logic for the "Library Folders" section within the settings dialog.
///
/// This struct encapsulates the `PreferencesGroup` and `ListBox` responsible for
/// displaying and allowing the removal of library folders. It holds references to
/// shared application state necessary for interacting with the database and
/// refreshing the main library UI.
pub struct FolderSettingsPage {
    pub folders_group: Rc<PreferencesGroup>,
    pub list_box: Rc<ListBox>,
    pub db_pool: Arc<SqlitePool>,
    pub refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    pub sort_ascending: Rc<Cell<bool>>,
    pub sort_ascending_artists: Rc<Cell<bool>>,
    pub main_context: Rc<MainContext>,
    pub sender: Option<UnboundedSender<()>>,
    pub scanning_label_albums: Rc<Label>,
    pub scanning_label_artists: Rc<Label>,
    pub albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    pub artists_stack_cell: Rc<RefCell<Option<Stack>>>,
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
        // Create UI components (delegated to ui_components module)
        let (folders_group, list_box, add_folder_btn) = create_folder_ui();

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

        // Connect event handlers
        connect_add_folder_handler(&add_folder_btn, folder_settings_page.clone());

        folder_settings_page
    }

    /// Returns a reference to the `PreferencesGroup` for this page, allowing it to be added
    /// to a `PreferencesPage`.
    pub fn group(&self) -> &PreferencesGroup {
        &self.folders_group
    }

    /// Creates a clone of `FolderSettingsPage` with its internal `Rc` and `Arc`
    /// fields also cloned. This is useful for passing the struct into closures
    /// without moving the original.
    pub fn clone_for_closure(&self) -> Self {
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
    pub async fn refresh_display(&self) {
        refresh_display(self).await;
    }
}

use std::{cell::Cell, rc::Rc, sync::Arc};

use glib::{MainContext, source::idle_add_local_once};
use gtk4::{Align::Center, Button, ListBox, SelectionMode::None, Window};
use libadwaita::{
    ActionRow, PreferencesGroup,
    prelude::{ActionRowExt, ButtonExt, Cast, PreferencesGroupExt, StaticType, WidgetExt},
};
use sqlx::SqlitePool;

use crate::{
    data::db::{cleanup::remove_folder_and_albums, query::fetch_all_folders},
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
    ) -> Self {
        let folders_group = PreferencesGroup::builder()
            .title("Library Folders")
            .description("Remove folders to exclude their music from your library.")
            .build();
        let list_box = ListBox::new();
        list_box.set_selection_mode(None);
        folders_group.add(&list_box);
        Self {
            folders_group: Rc::new(folders_group),
            list_box: Rc::new(list_box),
            db_pool,
            refresh_library_ui,
            sort_ascending,
            sort_ascending_artists,
            main_context,
        }
    }

    /// Returns a reference to the `PreferencesGroup` for this page, allowing it to be added
    /// to a `PreferencesPage`.
    pub fn group(&self) -> &PreferencesGroup {
        &self.folders_group
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

        // Clone the fetched folders for the closure
        let folders_c = folders.clone();
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

                    // Capture a clone of `self` (the FolderSettingsPage instance) for the closure.
                    // This allows the closure to access all the shared state (db_pool, refresh_library_ui, etc.)
                    // without needing to clone each field individually.
                    let self_c = Rc::new(Self {
                        folders_group: folders_group_c.clone(),
                        list_box: list_box_c.clone(),
                        db_pool: db_pool_c.clone(),
                        refresh_library_ui: refresh_library_ui_c.clone(),
                        sort_ascending: sort_ascending_c.clone(),
                        sort_ascending_artists: sort_ascending_artists_c.clone(),
                        main_context: main_context_c.clone(),
                    });
                    remove_btn.connect_clicked(move |btn| {
                        let parent_widget = btn
                            .ancestor(Window::static_type())
                            .expect("Button should be within a window heirarchy.");
                        let parent_window = parent_widget
                            .downcast_ref::<Window>()
                            .expect("Parent widget should be a window.");

                        // Clone `self_c` for the `on_confirm` closure of the dialog.
                        let self_dialog = self_c.clone();
                        show_remove_folder_confirmation_dialog(parent_window, move || {
                            // Spawn an asynchronous task on the main context.
                            self_dialog.main_context.spawn_local({
                                // Clone `self_dialog` for the async block itself.
                                let self_async = self_dialog.clone();
                                async move {
                                    // Perform database deletion.
                                    let _ =
                                        remove_folder_and_albums(&self_async.db_pool, folder_id)
                                            .await;

                                    // Refresh main library UI.
                                    (self_async.refresh_library_ui)(
                                        self_async.sort_ascending.get(),
                                        self_async.sort_ascending_artists.get(),
                                    );

                                    // Refresh the folder display in the settings dialog.
                                    self_async.refresh_display().await;
                                }
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

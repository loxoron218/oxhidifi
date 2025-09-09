use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::MainContext;
use gtk4::{Label, Stack};
use libadwaita::{PreferencesPage, prelude::PreferencesPageExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::settings::folder::FolderSettingsPage;

/// Creates and configures the Library preferences page.
///
/// This function sets up the Library page with folder management.
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
/// A configured `PreferencesPage` for library settings.
pub fn create_library_page(
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
) -> PreferencesPage {
    // Library page definition
    let library_page = PreferencesPage::builder()
        .title("Library")
        .icon_name("folder-music-symbolic")
        .build();

    // Initialize the FolderSettingsPage
    let folder_settings_page = FolderSettingsPage::new(
        db_pool.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        main_context.clone(),
        sender,
        scanning_label_albums,
        scanning_label_artists,
        albums_stack_cell,
        artists_stack_cell,
    );

    // Initial population of the folders group when the settings dialog opens.
    let folder_settings_page_clone = folder_settings_page.clone();
    main_context.spawn_local(async move {
        folder_settings_page_clone.refresh_display().await;
    });

    // Add folder group to the Library page.
    library_page.add(folder_settings_page.group());
    library_page
}

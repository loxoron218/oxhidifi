use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::MainContext;
use gtk4::ListBox;
use libadwaita::{
    PreferencesGroup, PreferencesPage,
    prelude::{PreferencesGroupExt, PreferencesPageExt},
};
use sqlx::SqlitePool;

use crate::ui::{
    components::sorting::{
        sorting_preferences::{
            connect_sort_reorder_handler, make_sort_row, update_sorting_row_numbers,
        },
        sorting_types::SortOrder,
    },
    settings::folder_settings::FolderSettingsPage,
};

/// Creates and configures the Library preferences page.
///
/// This function sets up the Library page with folder management and sorting preferences.
///
/// # Arguments
///
/// * `db_pool` - The database connection pool.
/// * `refresh_library_ui` - Callback to refresh the main library UI.
/// * `sort_ascending` - Shared state for album sort direction.
/// * `sort_ascending_artists` - Shared state for artist sort direction.
/// * `sort_orders` - Shared sort order preferences.
/// * `main_context` - The GLib main context for spawning UI tasks.
///
/// # Returns
///
/// A configured `PreferencesPage` for library settings.
pub fn create_library_page(
    db_pool: Arc<SqlitePool>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    main_context: Rc<MainContext>,
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
    );

    // Initial population of the folders group when the settings dialog opens.
    let folder_settings_page_clone = folder_settings_page.clone();
    main_context.spawn_local(async move {
        folder_settings_page_clone.refresh_display().await;
    });

    // Sorting group: Allows users to reorder sorting preferences.
    let sorting_group = PreferencesGroup::builder()
        .title("Sorting")
        .description("Albums will be sorted according to the order below. Drag to reorder.")
        .build();

    let sort_listbox = ListBox::new();

    // Disable selection for sort rows
    sort_listbox.set_selection_mode(gtk4::SelectionMode::None);
    sorting_group.add(&sort_listbox);

    // Populate the ListBox with ActionRows for each sort order.
    for order in sort_orders.borrow().iter() {
        let list_row = make_sort_row(
            order,
            sort_orders.clone(),
            refresh_library_ui.clone(),
            sort_ascending.clone(),
            sort_ascending_artists.clone(),
        );
        sort_listbox.append(&list_row);
    }

    // Update numbering (1., 2., etc.) for sort order rows.
    update_sorting_row_numbers(&sort_listbox);

    // Connect handler for reordering sort preferences via drag-and-drop.
    connect_sort_reorder_handler(&sort_listbox, sort_orders.clone());

    // Add folder and sorting groups to the Library page.
    library_page.add(folder_settings_page.group());
    library_page.add(&sorting_group);
    library_page
}

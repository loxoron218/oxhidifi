use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    EventControllerKey, Label, Stack, Widget, Window,
    gdk::Key,
    glib::{
        MainContext,
        Propagation::{Proceed, Stop},
    },
};
use libadwaita::{
    PreferencesDialog,
    prelude::{AdwDialogExt, IsA, PreferencesDialogExt, WidgetExt},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::{
        config::{load_settings, save_settings},
        view_controls::sorting_controls::types::SortOrder,
    },
    settings::{
        audio_page::create_audio_page, general_page::GeneralSettingsPage,
        library_page::create_library_page,
    },
};

/// Shows the settings dialog, providing an interface for users to manage application preferences.
///
/// This function constructs an `AdwPreferencesDialog` and populates it with various settings
/// pages and groups, including library folder management.
/// It interacts with shared application state to reflect and persist user choices.
///
/// # Arguments
///
/// * `parent` - The parent `gtk4::Window` for the settings dialog. For `AdwPreferencesDialog` to
///   function correctly, this should be an `adw::ApplicationWindow` or `adw::Window`.
/// * `sort_orders` - An `Rc<RefCell<Vec<SortOrder>>>` holding the current sort order preferences.
///   Changes made in the dialog are reflected in this shared state.
/// * `refresh_library_ui` - A callback `Rc<dyn Fn(bool, bool)>` to trigger a refresh of the
///   main library UI after settings changes (e.g., folder removal).
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the current sort direction for artists.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations, particularly for managing library folders.
/// * `is_settings_open` - An `Rc<Cell<bool>>` flag used to track whether the settings dialog is
///   currently open, preventing multiple instances.
/// * `show_dr_badges_setting` - An `Rc<Cell<bool>>` flag for showing DR badges.
/// * `use_original_year_setting` - An `Rc<Cell<bool>>` flag for using original release year.
/// * `show_album_metadata_setting` - An `Rc<Cell<bool>>` flag for showing album metadata.
/// * `sender` - Optional sender to notify UI refresh after scanning.
/// * `scanning_label_albums` - The scanning label for albums.
/// * `scanning_label_artists` - The scanning label for artists.
/// * `albums_stack_cell` - The albums stack cell.
/// * `artists_stack_cell` - The artists stack cell.
pub fn show_settings_dialog<P: IsA<Window> + IsA<Widget>>(
    parent: &P,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    db_pool: Arc<sqlx::SqlitePool>,
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
    // Create the settings dialog.
    let dialog = PreferencesDialog::new();

    // Set the content size. This replaces default_width and default_height.
    dialog.set_content_width(900);
    dialog.set_content_height(700);

    // Explicitly enable search, as it defaults to false in AdwPreferencesDialog.
    dialog.set_search_enabled(true);

    // Set flag to indicate settings dialog is open.
    is_settings_open.set(true);

    // Main GLib context for UI updates
    let main_context = Rc::new(MainContext::default());

    // Create the Library page
    let library_page = create_library_page(
        db_pool.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        main_context.clone(),
        sender,
        Rc::new(scanning_label_albums),
        Rc::new(scanning_label_artists),
        albums_stack_cell,
        artists_stack_cell,
    );

    // Create the General page
    let general_settings_page = GeneralSettingsPage::new(
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        show_dr_badges_setting.clone(),
        use_original_year_setting.clone(),
        show_album_metadata_setting.clone(),
    );
    let general_page = general_settings_page.create_page();

    // Create the Audio page
    let audio_page = create_audio_page();

    // Connect `closed` signal to save sort order when the settings dialog is closed.
    // This replaces the `close-request` signal from GtkWindow.
    let sort_orders_rc = sort_orders.clone();
    let is_settings_open_clone = is_settings_open.clone();
    let show_dr_badges_setting_clone_for_close = show_dr_badges_setting.clone();
    let use_original_year_setting_clone_for_close = use_original_year_setting.clone();
    let show_album_metadata_setting_clone_for_close = show_album_metadata_setting.clone();
    dialog.connect_closed(move |_| {
        let current_orders = sort_orders_rc.borrow().clone();
        let prev_settings = load_settings();
        let mut settings = load_settings();
        settings.sort_orders = current_orders;
        settings.sort_ascending_albums = prev_settings.sort_ascending_albums;
        settings.sort_ascending_artists = prev_settings.sort_ascending_artists;
        settings.best_dr_albums = prev_settings.best_dr_albums;
        settings.show_dr_badges = show_dr_badges_setting_clone_for_close.get();
        settings.use_original_year = use_original_year_setting_clone_for_close.get();
        settings.show_album_metadata = show_album_metadata_setting_clone_for_close.get();
        let _ = save_settings(&settings);
        is_settings_open_clone.set(false);
    });

    // --- Dialog-level interactions ---
    // Connect ESC key to close the dialog. AdwDialog handles this by default,
    // but an explicit controller is fine too. The `close()` method works on AdwDialog.
    let key_controller = EventControllerKey::new();
    {
        let dialog = dialog.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == Key::Escape {
                // Close the dialog
                dialog.close();

                // Stop further propagation of the event
                return Stop;
            }

            // Allow other key events to propagate
            Proceed
        });
    }

    // Add the key controller to the dialog
    dialog.add_controller(key_controller);

    // Add all defined pages to the preferences dialog.
    dialog.add(&general_page);
    dialog.add(&library_page);
    dialog.add(&audio_page);

    // Display the settings dialog within the parent window.
    // This is the main difference from the old `present()` method.
    dialog.present(Some(parent));
}

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    EventControllerKey, Label, Stack, Window,
    glib::{
        self,
        Propagation::{Proceed, Stop},
    },
};
use libadwaita::{
    PreferencesWindow,
    gdk::Key,
    prelude::{GtkWindowExt, IsA, PreferencesWindowExt, WidgetExt},
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
/// This function constructs a `PreferencesWindow` and populates it with various settings
/// pages and groups, including library folder management.
/// It interacts with shared application state to reflect and persist user choices.
///
/// # Arguments
///
/// * `parent` - The parent `gtk4::Window` for the settings dialog, making it modal.
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
/// * `sender` - Optional sender to notify UI refresh after scanning.
/// * `scanning_label_albums` - The scanning label for albums.
/// * `scanning_label_artists` - The scanning label for artists.
/// * `albums_stack_cell` - The albums stack cell.
/// * `artists_stack_cell` - The artists stack cell.
pub fn show_settings_dialog(
    parent: &impl IsA<Window>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    db_pool: Arc<sqlx::SqlitePool>,
    is_settings_open: Rc<Cell<bool>>,
    show_dr_badges_setting: Rc<Cell<bool>>,
    use_original_year_setting: Rc<Cell<bool>>,
    sender: Option<UnboundedSender<()>>,
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    artists_stack_cell: Rc<RefCell<Option<Stack>>>,
) {
    // Create the settings window, configured as a modal dialog.
    let dialog = PreferencesWindow::builder()
        .transient_for(parent)
        .default_width(900)
        .default_height(700)
        .modal(true)
        .build();

    // Set flag to indicate settings dialog is open.
    is_settings_open.set(true);

    // Apply margins for consistent spacing, matching GNOME HIG.
    dialog.set_margin_top(32);
    dialog.set_margin_bottom(32);
    dialog.set_margin_start(32);
    dialog.set_margin_end(32);

    // Main GLib context for UI updates
    let main_context = Rc::new(glib::MainContext::default());

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
        parent.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        show_dr_badges_setting.clone(),
        use_original_year_setting.clone(),
    );
    let general_page = general_settings_page.create_page();

    // Create the Audio page
    let audio_page = create_audio_page();

    // Connect `close-request` signal to save sort order when the settings window is closed.
    let sort_orders_rc = sort_orders.clone();
    let is_settings_open_clone = is_settings_open.clone();
    let show_dr_badges_setting_clone_for_close = show_dr_badges_setting.clone();
    let use_original_year_setting_clone_for_close = use_original_year_setting.clone();
    dialog.connect_close_request(move |_| {
        let current_orders = sort_orders_rc.borrow().clone();
        let prev_settings = load_settings();
        let mut settings = load_settings();
        settings.sort_orders = current_orders;
        settings.sort_ascending_albums = prev_settings.sort_ascending_albums;
        settings.sort_ascending_artists = prev_settings.sort_ascending_artists;
        settings.best_dr_albums = prev_settings.best_dr_albums;
        settings.show_dr_badges = show_dr_badges_setting_clone_for_close.get();
        settings.use_original_year = use_original_year_setting_clone_for_close.get();
        let _ = save_settings(&settings);
        is_settings_open_clone.set(false);
        Proceed
    });

    // --- Window-level interactions ---
    // Connect ESC key to close the dialog.
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

    // Add all defined pages to the preferences window.
    dialog.add(&general_page);
    dialog.add(&library_page);
    dialog.add(&audio_page);

    // Display the settings dialog to the user
    dialog.present();
}

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Box, EventControllerKey, Label,
    Orientation::Vertical,
    Stack, Window,
    glib::{
        MainContext,
        Propagation::{Proceed, Stop},
    },
};
use libadwaita::{
    HeaderBar, ViewStack, ViewSwitcher,
    ViewSwitcherPolicy::Wide,
    gdk::Key,
    prelude::{AdwWindowExt, BoxExt, GtkWindowExt, IsA, WidgetExt},
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
/// This function constructs a modern libadwaita window with HeaderBar and ViewStack
/// and populates it with various settings pages and groups, including library folder management.
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
/// * `is_settings_open` - An `Rc<Cell<bool>>` flag used to song whether the settings dialog is
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
    // Create the main window
    let window = libadwaita::Window::builder()
        .modal(true)
        .default_width(900)
        .default_height(700)
        .transient_for(parent)
        .title("Settings")
        .build();

    // Set flag to indicate settings dialog is open.
    is_settings_open.set(true);

    // Create header bar
    let header_bar = HeaderBar::builder().show_start_title_buttons(false).build();

    // Create main content container
    let main_box = Box::builder().orientation(Vertical).build();

    // Add header bar to main box
    main_box.append(&header_bar);

    // Create ViewStack for pages
    let view_stack = ViewStack::builder().build();

    // Create ViewSwitcher to put in the header bar
    let view_switcher = ViewSwitcher::builder()
        .stack(&view_stack)
        .policy(Wide)
        .build();

    // Set the view switcher as the title widget for the header bar
    header_bar.set_title_widget(Some(&view_switcher));

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
    );
    let general_page = general_settings_page.create_page();

    // Create the Audio page
    let audio_page = create_audio_page();

    // Connect `close-request` signal to save sort order when the settings window is closed.
    let sort_orders_rc = sort_orders.clone();
    let is_settings_open_clone = is_settings_open.clone();
    let show_dr_badges_setting_clone_for_close = show_dr_badges_setting.clone();
    let use_original_year_setting_clone_for_close = use_original_year_setting.clone();
    window.connect_close_request(move |_| {
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
        let window = window.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == Key::Escape {
                // Close the dialog
                window.close();

                // Stop further propagation of the event
                return Stop;
            }

            // Allow other key events to propagate
            Proceed
        });
    }

    // Add the key controller to the window
    window.add_controller(key_controller);

    // Add all defined pages to the ViewStack with icons
    view_stack.add_titled_with_icon(
        &general_page,
        Some("general"),
        "General",
        "preferences-system-symbolic",
    );
    view_stack.add_titled_with_icon(
        &library_page,
        Some("library"),
        "Library",
        "folder-music-symbolic",
    );
    view_stack.add_titled_with_icon(
        &audio_page,
        Some("audio"),
        "Audio",
        "audio-speakers-symbolic",
    );

    // Add the ViewStack to the main box
    main_box.append(&view_stack);

    // Set the content of the window
    window.set_content(Some(&main_box));

    // Display the settings dialog to the user
    window.present();
}

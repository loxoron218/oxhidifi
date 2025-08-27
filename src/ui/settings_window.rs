use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::{
    MainContext,
    Propagation::{Proceed, Stop},
    source::idle_add_local_once,
};
use gtk4::{
    Align::Center, Button, EventControllerKey, ListBox, SelectionMode::None, StringList,
    StringObject, Switch, Window,
};
use libadwaita::{
    ActionRow, ComboRow, PreferencesGroup, PreferencesPage, PreferencesWindow,
    gdk::Key,
    prelude::{
        ActionRowExt, ButtonExt, Cast, ComboRowExt, GtkWindowExt, IsA, ListModelExt,
        PreferencesGroupExt, PreferencesPageExt, PreferencesWindowExt, StaticType, WidgetExt,
    },
};
use sqlx::SqlitePool;

use crate::{
    data::db::{cleanup::remove_folder_and_albums, query::fetch_all_folders},
    ui::components::{
        config::{Settings, load_settings, save_settings},
        dialogs::{show_performance_metrics_dialog, show_remove_folder_confirmation_dialog},
        sorting::{
            sorting_preferences::{
                connect_sort_reorder_handler, make_sort_row, update_sorting_row_numbers,
            },
            sorting_types::SortOrder,
        },
    },
};

/// Manages the UI and logic for the "Library Folders" section within the settings dialog.
///
/// This struct encapsulates the `PreferencesGroup` and `ListBox` responsible for
/// displaying and allowing the removal of library folders. It holds references to
/// shared application state necessary for interacting with the database and
/// refreshing the main library UI.
struct FolderSettingsPage {
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
    fn new(
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
    fn group(&self) -> &PreferencesGroup {
        &self.folders_group
    }

    /// Refreshes the display of library folders by fetching them from the database
    /// and updating the `ListBox`.
    ///
    /// This method is asynchronous as it performs database queries. It handles
    /// the UI update on the main thread using `idle_add_local_once`.
    async fn refresh_display(&self) {
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
        let folders_c = folders.clone(); // Clone the fetched folders for the closure
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
                empty_row.add_css_class("dim-label"); // Apply styling for dimmed text
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
                    row.add_suffix(&remove_btn); // Add the remove button to the right of the row
                    list_box_c.append(&row); // Add the folder row to the ListBox
                }
            }
            // Request re-allocation and re-drawing of the ListBox.
            list_box_c.queue_allocate();
            list_box_c.queue_draw();
        });
    }
}

/// Shows the settings dialog, providing an interface for users to manage application preferences.
///
/// This function constructs a `PreferencesWindow` and populates it with various settings
/// pages and groups, including library folder management and sorting preferences.
/// It interacts with shared application state to reflect and persist user choices.
///
/// # Arguments
///
/// * `parent` - The parent `gtk4::Window` for the settings dialog, making it modal.
/// * `sort_orders` - An `Rc<RefCell<Vec<SortOrder>>>` holding the current sort order preferences.
///                   Changes made in the dialog are reflected in this shared state.
/// * `refresh_library_ui` - A callback `Rc<dyn Fn(bool, bool)>` to trigger a refresh of the
///                          main library UI after settings changes (e.g., folder removal, sort order change).
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the current sort direction for artists.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations, particularly for managing library folders.
/// * `is_settings_open` - An `Rc<Cell<bool>>` flag used to track whether the settings dialog is
///                        currently open, preventing multiple instances.
pub fn show_settings_dialog(
    parent: &impl IsA<Window>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    db_pool: Arc<SqlitePool>,
    is_settings_open: Rc<Cell<bool>>,
    show_dr_badges_setting: Rc<Cell<bool>>,
    use_original_year_setting: Rc<Cell<bool>>,
    view_mode_setting: Rc<RefCell<String>>,
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

    // --- Library Page: Contains folder management and sorting preferences ---
    // Library page definition
    let library_page = PreferencesPage::builder()
        .title("Library")
        .icon_name("folder-music-symbolic")
        .build();

    let main_context = Rc::new(MainContext::default()); // Main GLib context for UI updates

    // Initialize the FolderSettingsPage
    let folder_settings_page = Rc::new(FolderSettingsPage::new(
        db_pool.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        main_context.clone(),
    ));

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
    sort_listbox.set_selection_mode(None); // Disable selection for sort rows
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

    // Connect `close-request` signal to save sort order when the settings window is closed.
    let sort_orders_rc = sort_orders.clone();
    let is_settings_open_clone = is_settings_open.clone();
    let show_dr_badges_setting_clone_for_close = show_dr_badges_setting.clone();
    let use_original_year_setting_clone_for_close = use_original_year_setting.clone();
    let view_mode_setting_clone_for_close = view_mode_setting.clone();
    dialog.connect_close_request(move |_| {
        let current_orders = sort_orders_rc.borrow().clone();
        let prev_settings = load_settings();
        let _ = save_settings(&Settings {
            sort_orders: current_orders,
            sort_ascending_albums: prev_settings.sort_ascending_albums,
            sort_ascending_artists: prev_settings.sort_ascending_artists,
            completed_albums: prev_settings.completed_albums,
            show_dr_badges: show_dr_badges_setting_clone_for_close.get(),
            use_original_year: use_original_year_setting_clone_for_close.get(),
            view_mode: view_mode_setting_clone_for_close.borrow().to_string(),
        });
        is_settings_open_clone.set(false);
        Proceed
    });

    // Add folder and sorting groups to the Library page.
    library_page.add(folder_settings_page.group());
    library_page.add(&sorting_group);

    // --- General Page (Currently empty, but kept for potential future use) ---
    let general_page = PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();

    // Group for General settings
    let general_group = PreferencesGroup::builder().title("Display").build();

    // Group for Performance settings
    let performance_group = PreferencesGroup::builder().title("Performance").build();

    // Button to show performance metrics
    let performance_metrics_row = ActionRow::builder()
        .title("Performance Metrics")
        .subtitle("View detailed performance statistics and metrics.")
        .activatable(true)
        .build();
    let performance_metrics_button = Button::builder()
        .label("Show Metrics")
        .valign(Center)
        .build();
    performance_metrics_row.add_suffix(&performance_metrics_button);
    performance_metrics_row.set_activatable_widget(Some(&performance_metrics_button));

    // Clone necessary variables for the button click handler
    let parent_window_clone = parent.clone();
    performance_metrics_button.connect_clicked(move |_| {
        // We need to get the parent window for the dialog
        show_performance_metrics_dialog(parent_window_clone.as_ref());
    });
    performance_group.add(&performance_metrics_row);

    // Toggle switch for DR Value badges
    let dr_badges_row = ActionRow::builder()
        .title("Show DR Value Badges")
        .subtitle("Toggle the visibility of Dynamic Range (DR) Value badges.")
        .activatable(false)
        .build();
    let dr_badges_switch = Switch::builder()
        .valign(Center)
        .active(show_dr_badges_setting.get())
        .build();
    dr_badges_row.add_suffix(&dr_badges_switch);
    dr_badges_row.set_activatable_widget(Some(&dr_badges_switch));
    let show_dr_badges_setting_clone = show_dr_badges_setting.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    dr_badges_switch.connect_active_notify(move |switch| {
        show_dr_badges_setting_clone.set(switch.is_active());

        // Trigger a UI refresh to update the visibility of DR badges
        (refresh_library_ui_clone)(
            sort_ascending_clone.get(),
            sort_ascending_artists_clone.get(),
        );
    });
    general_group.add(&dr_badges_row);

    // Toggle switch for "Use Original Year"
    let use_original_year_row = ActionRow::builder()
        .title("Use Original Year for Albums")
        .subtitle("Display the original release year instead of the release year.")
        .activatable(false)
        .build();
    let use_original_year_switch = Switch::builder()
        .valign(Center)
        .active(use_original_year_setting.get())
        .build();
    use_original_year_row.add_suffix(&use_original_year_switch);
    use_original_year_row.set_activatable_widget(Some(&use_original_year_switch));
    let use_original_year_setting_clone = use_original_year_setting.clone();
    let refresh_library_ui_clone_for_year = refresh_library_ui.clone();
    let sort_ascending_clone_for_year = sort_ascending.clone();
    let sort_ascending_artists_clone_for_year = sort_ascending_artists.clone();
    use_original_year_switch.connect_active_notify(move |switch| {
        use_original_year_setting_clone.set(switch.is_active());

        // Trigger a UI refresh to update the year display
        (refresh_library_ui_clone_for_year)(
            sort_ascending_clone_for_year.get(),
            sort_ascending_artists_clone_for_year.get(),
        );
    });
    general_group.add(&use_original_year_row);

    // ComboRow for View Mode
    let view_mode_row = ComboRow::builder()
        .title("View Mode")
        .subtitle("Choose how albums and artists are displayed.")
        .build();
    let view_options = StringList::new(&["Grid View", "List View"]);
    view_mode_row.set_model(Some(&view_options));

    // Set default selection based on current setting
    let initial_view_mode_index = match view_mode_setting.borrow().as_str() {
        "Grid View" => 0,
        _ => 1,
    };
    view_mode_row.set_selected(initial_view_mode_index);
    let view_mode_setting_clone = view_mode_setting.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    view_mode_row.connect_selected_notify(move |combo_row| {
        let selected_index = combo_row.selected();
        let selected_item = view_options
            .item(selected_index)
            .and_then(|obj| obj.downcast::<StringObject>().ok())
            .map(|s_obj| s_obj.string().to_string());
        if let Some(mode) = selected_item {
            *view_mode_setting_clone.borrow_mut() = mode;
            (refresh_library_ui_clone)(
                sort_ascending_clone.get(),
                sort_ascending_artists_clone.get(),
            );
        }
    });
    general_group.add(&view_mode_row);
    general_page.add(&general_group);
    general_page.add(&performance_group);

    // --- Audio Page (Currently empty, but kept for potential future use) ---
    let audio_page = PreferencesPage::builder()
        .title("Audio")
        .icon_name("audio-speakers-symbolic")
        .build();

    // --- Window-level interactions ---
    // Connect ESC key to close the dialog.
    let key_controller = EventControllerKey::new();
    {
        let dialog = dialog.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == Key::Escape {
                dialog.close(); // Close the dialog
                return Stop; // Stop further propagation of the event
            }
            Proceed // Allow other key events to propagate
        });
    }
    dialog.add_controller(key_controller); // Add the key controller to the dialog

    // Add all defined pages to the preferences window.
    dialog.add(&general_page);
    dialog.add(&library_page);
    dialog.add(&audio_page);
    dialog.present(); // Display the settings dialog to the user
}

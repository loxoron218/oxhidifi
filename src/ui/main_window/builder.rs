use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    thread,
};

use gtk4::{Box, FlowBox, Orientation, Stack};
use libadwaita::{
    Application, ApplicationWindow, Clamp, ViewStack,
    prelude::{AdwApplicationWindowExt, BoxExt, ButtonExt, GtkWindowExt},
};
use sqlx::SqlitePool;
use tokio::runtime::Runtime;

use crate::{
    data::scanner::library_ops::run_full_scan,
    ui::{
        components::{
            config::load_settings, refresh::setup_library_refresh_channel,
            scan_feedback::create_scanning_label,
        },
        header::{build_header_bar, build_main_headerbar, build_tab_bar},
    },
    utils::screen::ScreenInfo,
};

use super::{handlers::connect_all_handlers, state::WindowSharedState, widgets::WindowWidgets};

/// Build and present the main application window, including all UI widgets, search, and navigation.
/// Handles all top-level UI logic, event connections, and async refresh flows.
///
/// This function orchestrates the creation of the main application window, its header bar,
/// content areas, and connects various UI events to their respective handlers. It initializes
/// the shared state for the application, sets up data binding for UI updates, and
/// initiates background tasks like library scanning and file system watching.
///
/// # Arguments
///
/// * `app` - The `libadwaita::Application` instance, representing the GTK application.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations, shared across the application.
pub fn build_main_window(app: &Application, db_pool: Arc<SqlitePool>) {
    // Initialize core GTK widgets and application window
    // The `ApplicationWindow` is the top-level window for the application.
    let window = ApplicationWindow::builder()
        .application(app)
        .title("oxhidifi")
        .default_width(1200)
        .default_height(800)
        .maximized(false)
        .build();

    // Build header bar components: main header bar and tab bar (Albums/Artists)
    // The `AppHeaderBar` struct contains all the buttons and search bar.
    // The tab bar contains the Albums and Artists toggle buttons.
    let app_header_bar_widgets = build_header_bar();
    let (tab_bar, albums_btn, artists_btn) = build_tab_bar();

    // Initialize main content `ViewStack` and scanning indicators
    // The `ViewStack` allows switching between different main views (e.g., Albums grid, Artists grid).
    let stack = ViewStack::builder().vexpand(true).hexpand(true).build();
    let scanning_label_albums = create_scanning_label();
    let scanning_label_artists = create_scanning_label();

    // Initialize shared state for sorting, navigation, and dynamic sizing
    // Load persistent user settings for initial sort orders.
    let settings = load_settings();
    // Get primary screen dimensions to calculate optimal cover and tile sizes dynamically.
    let screen_info = ScreenInfo::new();

    // `WindowSharedState` aggregates all `Rc<Cell<T>>` and `Rc<RefCell<T>>` managed state.
    // This centralizes mutable state management, making it easier to reason about data flow.
    let shared_state = WindowSharedState {
        sort_orders: Rc::new(RefCell::new(settings.sort_orders)),
        sort_ascending: Rc::new(Cell::new(settings.sort_ascending_albums)),
        sort_ascending_artists: Rc::new(Cell::new(settings.sort_ascending_artists)),
        last_tab: Rc::new(Cell::new("albums")), // Tracks the last active main tab (Albums or Artists).
        nav_history: Rc::new(RefCell::new(Vec::new())), // Stores navigation history for back functionality.
        screen_info: Rc::new(RefCell::new(screen_info)),
        is_settings_open: Rc::new(Cell::new(false)), // Flag to prevent UI refresh while settings dialog is open.
    };

    // Initialize `Rc<RefCell<Option<FlowBox>>>` and `Rc<RefCell<Option<Stack>>>` for grids and stacks
    // These `Rc<RefCell<Option<...>>>` are used to hold references to the `FlowBox` (grids) and
    // their containing `Stack` widgets. This allows them to be dynamically updated and
    // passed around in a thread-safe manner within the GTK main context.
    let albums_grid_cell: Rc<RefCell<Option<FlowBox>>> = Rc::new(RefCell::new(None));
    let albums_stack_cell: Rc<RefCell<Option<Stack>>> = Rc::new(RefCell::new(None));
    let artist_grid_cell: Rc<RefCell<Option<FlowBox>>> = Rc::new(RefCell::new(None));
    let artists_stack_cell: Rc<RefCell<Option<Stack>>> = Rc::new(RefCell::new(None));

    // Bundle all static widgets into `WindowWidgets` struct for cleaner passing
    // This struct holds references to all the GTK widgets that are created once and
    // remain static throughout the application's lifetime.
    let widgets = WindowWidgets {
        window: window.clone(),
        stack: stack.clone(),
        left_btn_stack: app_header_bar_widgets.left_btn_stack.clone(),
        right_btn_box: app_header_bar_widgets.right_btn_box.clone(),
        add_button: app_header_bar_widgets.add_button.clone(),
        back_button: app_header_bar_widgets.back_button.clone(),
        settings_button: app_header_bar_widgets.settings_button.clone(),
        search_bar: app_header_bar_widgets.clone(), // Clone the entire AppHeaderBar struct
        sort_button: app_header_bar_widgets.sort_button.clone(),
        albums_btn: albums_btn.clone(),
        artists_btn: artists_btn.clone(),
        scanning_label_albums: scanning_label_albums.clone(),
        scanning_label_artists: scanning_label_artists.clone(),
        albums_grid_cell: albums_grid_cell.clone(),
        albums_stack_cell: albums_stack_cell.clone(),
        artist_grid_cell: artist_grid_cell.clone(),
        artists_stack_cell: artists_stack_cell.clone(),
    };

    // Setup library refresh channel and service
    // `setup_library_refresh_channel` creates an MPSC channel for triggering UI refreshes
    // and returns a `RefreshService` instance that encapsulates the refresh logic
    // and all necessary UI components.
    let (sender, receiver, refresh_library_ui, refresh_service) = setup_library_refresh_channel(
        db_pool.clone(),
        widgets.albums_grid_cell.clone(),
        widgets.albums_stack_cell.clone(),
        widgets.artist_grid_cell.clone(),
        widgets.artists_stack_cell.clone(),
        shared_state.sort_orders.clone(),
        Rc::new(widgets.stack.clone()),
        Rc::new(widgets.left_btn_stack.clone()),
        widgets.right_btn_box.clone(),
        shared_state.screen_info.clone(),
        shared_state.sort_ascending.clone(),
        shared_state.sort_ascending_artists.clone(),
        widgets.window.clone(),
        widgets.scanning_label_albums.clone(),
        widgets.scanning_label_artists.clone(),
        Rc::new(app_header_bar_widgets.left_btn_stack.clone()), // Use from the original header struct
        shared_state.nav_history.clone(),
    );

    // Build the main `gtk4::HeaderBar` by composing its left, center (tab bar), and right sections.
    // The header bar provides primary navigation and actions for the application.
    let center_inner = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    center_inner.append(&tab_bar);
    let center_box = Clamp::builder().child(&center_inner).build();

    // Artists toggle button frame is removed for a cleaner, tab-like appearance.
    widgets.artists_btn.set_has_frame(false);

    let header_bar = build_main_headerbar(
        &widgets.search_bar.left_btn_stack, // Use the left_btn_stack from the cloned AppHeaderBar
        &widgets.search_bar.right_btn_box,  // Use the right_btn_box from the cloned AppHeaderBar
        &center_box,
    );

    // The main vertical box arranges the header bar at the top and the content `ViewStack` below it.
    let vbox_inner = Box::new(Orientation::Vertical, 0);
    vbox_inner.append(&header_bar);
    vbox_inner.append(&widgets.stack);

    // Connect all the handlers
    connect_all_handlers(
        &widgets,
        &shared_state,
        db_pool.clone(),
        sender.clone(),
        receiver, // receiver is moved here
        refresh_library_ui.clone(),
        refresh_service.clone(),
        &vbox_inner,
    );

    // Present the window to make it visible and set its content.
    // This is the final step in rendering the main application window.
    widgets.window.present();
    widgets.window.set_content(Some(&vbox_inner));

    // Initiate an initial full scan on application startup in a separate thread.
    // This is a non-blocking operation that populates the library with existing music files,
    // ensuring the UI remains responsive during the initial data loading process.
    let db_pool_startup_scan = db_pool.clone();
    let sender_startup_scan = sender.clone();
    thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            run_full_scan(&db_pool_startup_scan, &sender_startup_scan).await;
        });
    });
}

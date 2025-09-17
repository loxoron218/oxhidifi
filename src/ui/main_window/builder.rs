use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    thread,
};

use gtk4::{
    Align::{Center, End},
    Box, Button, FlowBox, Label,
    Orientation::{Horizontal, Vertical},
    Overlay, Stack,
};
use libadwaita::{
    Application, ApplicationWindow, Clamp, ViewStack,
    prelude::{AdwApplicationWindowExt, BoxExt, ButtonExt, GtkWindowExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::runtime::Runtime;

use crate::{
    data::scanner::library_ops::run_full_scan,
    ui::{
        components::{
            config::{load_settings, save_settings},
            player_bar::PlayerBar,
            refresh::setup_library_refresh_channel,
            scan_feedback::create_scanning_label,
            view_controls::{
                ZoomLevel, ZoomManager,
                list_view::column_view::{
                    zoom::ColumnViewZoomLevel::Normal, zoom_manager::ColumnViewZoomManager,
                },
            },
        },
        grids::{
            album_grid_rebuilder::rebuild_albums_grid_for_window,
            artist_grid_builder::build_artist_grid,
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

    // Create "Add Music" buttons for empty states.
    let add_music_button_albums = Button::with_label("Add Music");
    let add_music_button_artists = Button::with_label("Add Music");

    // Create the labels for displaying album and artist counts.
    let album_count_label = Rc::new(
        Label::builder()
            .label("0 Albums")
            .halign(Center)
            .margin_top(12)
            .css_classes(&*["dim-label"].as_ref())
            .build(),
    );
    let artist_count_label = Rc::new(
        Label::builder()
            .label("0 Artists")
            .halign(Center)
            .margin_top(12)
            .css_classes(&*["dim-label"].as_ref())
            .build(),
    );

    // Initialize shared state for sorting, navigation, and dynamic sizing
    // Load persistent user settings for initial sort orders.
    let settings = load_settings();

    // Get primary screen dimensions to calculate optimal cover and tile sizes dynamically.
    let screen_info = ScreenInfo::new();

    // Create the player bar
    let player_bar = PlayerBar::new();

    // Create the zoom managers
    let zoom_manager = Rc::new(ZoomManager::new(settings.current_zoom_level));
    let column_view_zoom_manager = Rc::new(ColumnViewZoomManager::new(Normal));

    // Update screen info with the loaded zoom level values if not Medium
    let screen_info = if settings.current_zoom_level != ZoomLevel::Medium {
        let mut screen_info = screen_info;
        let cover_size = settings.current_zoom_level.cover_size();
        let tile_size = settings.current_zoom_level.tile_size();
        screen_info.update_with_zoom(cover_size, tile_size);
        screen_info
    } else {
        screen_info
    };

    // `WindowSharedState` aggregates all `Rc<Cell<T>>` and `Rc<RefCell<T>>` managed state.
    // This centralizes mutable state management, making it easier to reason about data flow.
    let shared_state = WindowSharedState {
        sort_orders: Rc::new(RefCell::new(settings.sort_orders)),
        sort_ascending: Rc::new(Cell::new(settings.sort_ascending_albums)),
        sort_ascending_artists: Rc::new(Cell::new(settings.sort_ascending_artists)),
        last_tab: Rc::new(Cell::new("albums")),
        nav_history: Rc::new(RefCell::new(Vec::new())),
        screen_info: Rc::new(RefCell::new(screen_info)),
        is_settings_open: Rc::new(Cell::new(false)),
        show_dr_badges: Rc::new(Cell::new(settings.show_dr_badges)),
        use_original_year: Rc::new(Cell::new(settings.use_original_year)),
        zoom_manager,
        column_view_zoom_manager,
        current_zoom_level: Rc::new(Cell::new(settings.current_zoom_level)),
        current_view_mode: Rc::new(Cell::new(settings.view_mode)),
        initial_scan_ongoing: Rc::new(Cell::new(true)),
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
        back_button: app_header_bar_widgets.back_button.clone(),
        settings_button: app_header_bar_widgets.settings_button.clone(),
        search_bar: app_header_bar_widgets.clone(),
        button: app_header_bar_widgets.button.clone(),
        albums_btn: albums_btn.clone(),
        artists_btn: artists_btn.clone(),
        scanning_label_albums: scanning_label_albums.clone(),
        scanning_label_artists: scanning_label_artists.clone(),
        album_count_label: album_count_label.clone(),
        artist_count_label: artist_count_label.clone(),
        albums_grid_cell: albums_grid_cell.clone(),
        albums_stack_cell: albums_stack_cell.clone(),
        artist_grid_cell: artist_grid_cell.clone(),
        artists_stack_cell: artists_stack_cell.clone(),
        player_bar: player_bar.clone(),
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
        widgets.scanning_label_albums.clone(),
        widgets.scanning_label_artists.clone(),
        widgets.album_count_label.clone(),
        widgets.artist_count_label.clone(),
        shared_state.nav_history.clone(),
        shared_state.show_dr_badges.clone(),
        shared_state.use_original_year.clone(),
        widgets.player_bar.clone(),
        widgets.window.clone().into(),
        Some(shared_state.current_zoom_level.clone()),
    );

    // Get the albums stack from the cell (always available)
    let _albums_stack = albums_stack_cell.borrow().as_ref().cloned();

    // Build the album grid
    // Load persistent user settings for initial sort orders and view mode.
    let settings = load_settings();
    let _model = rebuild_albums_grid_for_window(
        &widgets.stack,
        &widgets.scanning_label_albums,
        &shared_state.screen_info,
        &albums_grid_cell,
        &albums_stack_cell,
        &widgets.window.clone().into(),
        &db_pool,
        &sender,
        widgets.album_count_label.clone(),
        settings.view_mode,
        settings.use_original_year,
        shared_state.show_dr_badges.clone(),
        Some(refresh_service.clone()),
        Some(shared_state.column_view_zoom_manager.clone()),
    );

    // Build the artist grid
    let (artists_stack, artist_grid) = build_artist_grid(
        &widgets.scanning_label_artists,
        &add_music_button_artists,
        widgets.artist_count_label.clone(),
    );

    // Update zoom controls based on initial view mode
    widgets.button.set_view_mode(settings.view_mode);

    // Set the initial children of the `ViewStack` to the newly built album and artist stacks.
    // The albums stack was already added by rebuild_albums_grid_for_window
    widgets
        .stack
        .add_titled(&artists_stack, Some("artists"), "Artists");

    // Store the artist grid and stack in the cells
    artist_grid_cell.borrow_mut().replace(artist_grid);
    artists_stack_cell.borrow_mut().replace(artists_stack);

    // Build the main `gtk4::HeaderBar` by composing its left, center (tab bar), and right sections.
    // The header bar provides primary navigation and actions for the application.
    let center_inner = Box::builder().orientation(Horizontal).spacing(6).build();
    center_inner.append(&tab_bar);
    let center_box = Clamp::builder().child(&center_inner).build();

    // Artists toggle button frame is removed for a cleaner, tab-like appearance.
    widgets.artists_btn.set_has_frame(false);
    let header_bar = build_main_headerbar(
        &widgets.search_bar.left_btn_stack,
        &widgets.search_bar.right_btn_box,
        &center_box,
    );

    // The main vertical box arranges the header bar at the top and the content `ViewStack` below it.
    let vbox_inner = Box::new(Vertical, 0);
    vbox_inner.append(&header_bar);
    vbox_inner.append(&widgets.stack);

    // Create the overlay and add the main content and player bar
    let overlay = Overlay::new();
    overlay.set_child(Some(&vbox_inner));
    overlay.add_overlay(&player_bar.container);
    player_bar.container.set_valign(End);

    // Set the main content area for the player bar and connect visibility changes
    let mut player_bar_mut = player_bar.clone();
    player_bar_mut.set_main_content_area(vbox_inner.clone());
    player_bar_mut.connect_visibility_changes();

    // Connect the zoom managers to the view control button
    widgets.button.set_zoom_managers(
        shared_state.zoom_manager.clone(),
        shared_state.column_view_zoom_manager.clone(),
    );

    // Update zoom controls after connecting zoom managers
    widgets.button.update_zoom_controls();

    // Connect all the handlers
    connect_all_handlers(
        &widgets,
        &shared_state,
        db_pool.clone(),
        sender.clone(),
        receiver,
        refresh_library_ui.clone(),
        refresh_service.clone(),
        &vbox_inner,
        &add_music_button_albums,
        &add_music_button_artists,
    );

    // Connect the zoom manager callback to update screen info and rebuild grids
    let screen_info_clone = shared_state.screen_info.clone();
    let current_zoom_level_clone = shared_state.current_zoom_level.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    shared_state
        .zoom_manager
        .connect_zoom_changed(move |zoom_level| {
            // Store the current zoom level
            current_zoom_level_clone.set(zoom_level);

            // Save the new zoom level to settings
            let mut settings = load_settings();
            settings.current_zoom_level = zoom_level;
            let _ = save_settings(&settings);

            // Update screen info with new zoom level values
            if zoom_level == ZoomLevel::Medium {
                // Reset to original screen dimensions for default zoom level
                screen_info_clone.borrow_mut().reset_to_original();
            } else {
                // Use fixed values for other zoom levels
                let cover_size = zoom_level.cover_size();
                let tile_size = zoom_level.tile_size();
                screen_info_clone
                    .borrow_mut()
                    .update_with_zoom(cover_size, tile_size);
            }

            // Trigger a UI refresh to rebuild the grids with new sizes
            refresh_library_ui_clone(true, true);
        });

    // Connect the column view zoom manager callback to update column widths and cover sizes
    let refresh_library_ui_clone = refresh_library_ui.clone();
    shared_state
        .column_view_zoom_manager
        .connect_zoom_changed(move |_zoom_level| {
            // Trigger a UI refresh to update the column view with new sizes
            refresh_library_ui_clone(true, true);
        });

    // Set the initial view to albums since last_tab is set to "albums"
    widgets.stack.set_visible_child_name("albums");

    // Present the window to make it visible and set its content.
    // This is the final step in rendering the main application window.
    widgets.window.present();
    widgets.window.set_content(Some(&overlay));

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

use std::{rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use glib::WeakRef;
use gtk4::{Box, FlowBox, Orientation, Stack};
use libadwaita::{Application, ApplicationWindow, Clamp, ViewStack};
use libadwaita::prelude::{AdwApplicationWindowExt, BoxExt, ButtonExt, GtkWindowExt};
use sqlx::SqlitePool;

use crate::data::scanner::{connect_rescan_button, create_scanning_label, connect_scanning_label_visibility, spawn_scanning_label_refresh_task};
use crate::data::search::connect_live_search;
use crate::ui::components::config::load_settings;
use crate::ui::components::dialogs::{connect_add_folder_dialog, connect_settings_dialog};
use crate::ui::components::navigation::{connect_album_navigation, connect_back_button, connect_sort_button, connect_tab_navigation, setup_keyboard_shortcuts};
use crate::ui::components::refresh::{setup_library_refresh_channel, setup_live_monitor_refresh};
use crate::ui::components::sorting::{connect_sort_icon_update_on_tab_switch, connect_tab_sort_refresh, set_initial_sort_icon_state};
use crate::ui::grids::albums_grid::rebuild_albums_grid_for_window;
use crate::ui::grids::artists_grid::rebuild_artists_grid_for_window;
use crate::ui::header::{build_header_bar, build_main_headerbar, build_tab_bar};
use crate::ui::pages::album_page::album_page;
use crate::ui::search_bar::{connect_searchbar_focus_out, setup_searchbar_all};
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

/// Build and present the main application window, including all UI widgets, search, and navigation.
/// Handles all top-level UI logic, event connections, and async refresh flows.
pub fn build_main_window(app: &Application, db_pool: Arc<SqlitePool>) {

    // Header bar and state
    let header = build_header_bar();
    let left_btn_stack = header.left_btn_stack.clone();
    let left_btn_stack_weak = WeakRef::<ViewStack>::new();
    left_btn_stack_weak.set(Some(&left_btn_stack));
    let right_btn_box = header.right_btn_box.clone();
    let add_button = header.add_button.clone();
    let rescan_button = header.rescan_button.clone();
    let back_button = header.back_button.clone();
    let settings_button = header.settings_button.clone();
    let search_bar = header.search_bar.clone();
    let sort_button = header.sort_button.clone();

    // Load persistent settings for sort directions
    let settings = load_settings();
    let sort_orders = Rc::new(RefCell::new(settings.sort_orders));
    let sort_ascending = Rc::new(Cell::new(settings.sort_ascending_albums));
    let sort_ascending_artists = Rc::new(Cell::new(settings.sort_ascending_artists));

    // Set initial sort icon state via modular function
    set_initial_sort_icon_state(
        &sort_button,
        &sort_ascending,
        &sort_ascending_artists,
        "albums",
    );

    // Main content stack
    let stack = ViewStack::builder()
        .vexpand(true)
        .hexpand(true)
        .build();
    let stack_rc = Rc::new(stack.clone());
    let left_btn_stack_rc = Rc::new(left_btn_stack.clone());
    let last_tab = Rc::new(Cell::new("albums"));
    let nav_history = Rc::new(RefCell::new(Vec::new()));

    // Navigation
    connect_back_button(
        &back_button,
        &stack,
        &left_btn_stack,
        &right_btn_box,
        last_tab.clone(),
        nav_history.clone(),
    );

    // Scanning indicators
    let scanning_label_albums = create_scanning_label();
    let scanning_label_artists = create_scanning_label();

    // Store cover/tile size in Rc<Cell<i32>> for live update
    let (screen_width, _screen_height) = get_primary_screen_size();
    let (cover_size, tile_size) = compute_cover_and_tile_size(screen_width);
    let cover_size_rc = Rc::new(Cell::new(cover_size));
    let tile_size_rc = Rc::new(Cell::new(tile_size));

    // Robust grid and stack storage using Rc<RefCell<Option<FlowBox>>> and Rc<RefCell<Option<Stack>>>
    let albums_grid_cell: Rc<RefCell<Option<FlowBox>>> = Rc::new(RefCell::new(None));
    let albums_stack_cell: Rc<RefCell<Option<Stack>>> = Rc::new(RefCell::new(None));
    let artists_grid_cell: Rc<RefCell<Option<FlowBox>>> = Rc::new(RefCell::new(None));
    let artists_stack_cell: Rc<RefCell<Option<Stack>>> = Rc::new(RefCell::new(None));

    // Albums grid (modularized)
    rebuild_albums_grid_for_window(
        &stack,
        &scanning_label_albums,
        &cover_size_rc,
        &tile_size_rc,
        &albums_grid_cell,
        &albums_stack_cell,
    );

    // Artists grid (modularized)
    rebuild_artists_grid_for_window(
        &stack,
        &scanning_label_artists,
        &artists_grid_cell,
        &artists_stack_cell,
    );
    let window = ApplicationWindow::builder()
        .application(app)
        .title("oxhidifi")
        .default_width(1200)
        .default_height(800)
        .maximized(false)
        .build();

    // Library refresh logic is now modularized in refresh.rs
    let (sender, receiver, refresh_library_ui) = setup_library_refresh_channel(
        db_pool.clone(),
        albums_grid_cell.clone(),
        albums_stack_cell.clone(),
        artists_grid_cell.clone(),
        artists_stack_cell.clone(),
        sort_orders.clone(),
        stack_rc.clone(),
        left_btn_stack_rc.clone(),
        right_btn_box.clone(),
        cover_size_rc.clone(),
        tile_size_rc.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        window.clone(),
        scanning_label_albums.clone(),
        scanning_label_artists.clone(),
        stack.clone().into(),
        header.left_btn_stack.clone().into(),
        nav_history.clone(),
    );
    setup_live_monitor_refresh(
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        cover_size_rc.clone(),
        tile_size_rc.clone(),
        refresh_library_ui.clone(),
        screen_width,
    );

    // Navigation logic
    // Initial connect for album navigation (will also be called after grid rebuild)
    if let Some(albums_grid) = albums_grid_cell.borrow().as_ref() {
        connect_album_navigation(
            albums_grid,
            &stack,
            db_pool.clone(),
            &left_btn_stack,
            &right_btn_box,
            nav_history.clone(),
            |stack_weak, db_pool, album_id, left_btn_stack_weak| async move {
                album_page(stack_weak, db_pool, album_id, left_btn_stack_weak).await;
            },
        );
    }

    // Tab bar
    let (tab_bar, albums_btn, artists_btn) = build_tab_bar();

    // Tab navigation
    connect_tab_navigation(
        &albums_btn,
        &artists_btn,
        &stack,
        &sort_button,
        last_tab.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        refresh_library_ui.clone(),
        None::<fn()>,
    );

    // Sorting logic for tab toggles and sort icon
    connect_tab_sort_refresh(
        &albums_btn,
        &artists_btn,
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );
    connect_sort_icon_update_on_tab_switch(
        &sort_button,
        &stack,
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );

    // Sort button logic (persist sort direction)
    connect_sort_button(
        &sort_button,
        &stack,
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        refresh_library_ui.clone(),
    );

    // Scanning label and refresh after scan
    let receiver = Rc::new(RefCell::new(receiver));
    spawn_scanning_label_refresh_task(
        receiver,
        scanning_label_albums.clone(),
        scanning_label_artists.clone(),
        stack.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );

    // Rescan button logic
    {
        let scanning_label_albums = scanning_label_albums.clone();
        let scanning_label_artists = scanning_label_artists.clone();
        let stack = stack.clone();
        connect_rescan_button(
            &rescan_button,
            scanning_label_albums.clone(),
            sender.clone(),
            db_pool.clone(),
        );
        connect_scanning_label_visibility(
            &rescan_button,
            &stack,
            &scanning_label_albums,
            &scanning_label_artists,
        );
    }

    // Center box for tabs
    let center_inner = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    center_inner.append(&tab_bar);
    let center_box = Clamp::builder().child(&center_inner).build();

    // Artists toggle button
    artists_btn.set_has_frame(false);

    // Build header bar
    let header_bar = build_main_headerbar(
        &header.left_btn_stack,
        &header.right_btn_box,
        &center_box,
    );

    // Live search
    connect_live_search(
        &search_bar.entry,
        albums_grid_cell.borrow().as_ref().unwrap(),
        albums_stack_cell.borrow().as_ref().unwrap(),
        artists_grid_cell.borrow().as_ref().unwrap(),
        artists_stack_cell.borrow().as_ref().unwrap(),
        db_pool.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        refresh_library_ui.clone(),
        stack.clone().into(),
        left_btn_stack.clone().into(),
        Rc::new(right_btn_box.clone()),
        nav_history.clone(),
    );

    // Search bar focus out
    connect_searchbar_focus_out(&search_bar);

    // Back button navigation (again, for redundancy)
    connect_back_button(
        &back_button,
        &stack,
        &left_btn_stack,
        &right_btn_box,
        last_tab.clone(),
        nav_history.clone(),
    );

    // Window construction
    let vbox_inner = Box::new(Orientation::Vertical, 0);
    vbox_inner.append(&header_bar);
    vbox_inner.append(&stack);

    // Set up all search bar UI logic (gesture, show/hide, focus, keys)
    setup_searchbar_all(
        &search_bar,
        &window,
        &vbox_inner,
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );

    // Keyboard shortcuts and ESC navigation
    setup_keyboard_shortcuts(
        &window,
        &search_bar,
        &refresh_library_ui,
        &sort_ascending,
        &sort_ascending_artists,
        &stack,
        &left_btn_stack,
        &right_btn_box,
        &last_tab,
        &nav_history,
    );

    // Add folder dialog
    connect_add_folder_dialog(
        &add_button,
        window.clone(),
        scanning_label_albums.clone(),
        db_pool.clone(),
        sender.clone(),
    );

    // Settings dialog
    connect_settings_dialog(
        &settings_button,
        window.clone(),
        sort_orders.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        db_pool.clone(),
    );

    // Present the main window and set its main content container for the UI layout
    window.present();
    window.set_content(Some(&vbox_inner));
}

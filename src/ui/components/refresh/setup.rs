use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gtk4::{
    FlowBox, Label, Stack, Window,
    glib::{ControlFlow::Continue, source::timeout_add_local},
};
use libadwaita::{Clamp, ViewStack};
use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    ui::components::{
        player_bar::PlayerBar,
        refresh::{
            AlbumsUIComponents, ArtistsUIComponents, DisplaySettings, NavigationComponents,
            RefreshService, SortingComponents,
        },
        view_controls::{ZoomLevel, sorting_controls::types::SortOrder},
    },
    utils::screen::ScreenInfo,
};

/// Sets up the library refresh channel and the refresh UI closure.
/// This function is the primary entry point for initializing the refresh mechanism.
///
/// Returns a tuple containing:
/// - `UnboundedSender<()>`: A sender to trigger UI refreshes from other parts of the application.
/// - `UnboundedReceiver<()>`: A receiver for the refresh signals.
/// - `Rc<dyn Fn(bool, bool)>`: A refresh closure that can be called to explicitly refresh the UI.
pub fn setup_library_refresh_channel(
    db_pool: Arc<SqlitePool>,
    albums_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    artist_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    artists_stack_cell: Rc<RefCell<Option<Stack>>>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    stack: Rc<ViewStack>,
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Clamp,
    screen_info: Rc<RefCell<ScreenInfo>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    album_count_label: Rc<Label>,
    artist_count_label: Rc<Label>,
    nav_history: Rc<RefCell<Vec<String>>>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    window: Window,
    current_zoom_level: Option<Rc<Cell<ZoomLevel>>>,
) -> (
    UnboundedSender<()>,
    UnboundedReceiver<()>,
    Rc<dyn Fn(bool, bool)>,
    Rc<RefreshService>,
) {
    let (sender, receiver) = unbounded_channel::<()>();

    // Create grouping structs
    let albums_components = AlbumsUIComponents {
        grid_cell: albums_grid_cell,
        stack_cell: albums_stack_cell,
        scanning_label: scanning_label_albums,
        count_label: album_count_label,
    };

    // Group UI components related to artists display (grid, stack, labels)
    let artists_components = ArtistsUIComponents {
        grid_cell: artist_grid_cell,
        stack_cell: artists_stack_cell,
        scanning_label: scanning_label_artists,
        count_label: artist_count_label,
    };

    // Group navigation-related UI components (stacks, buttons, history)
    let navigation_components = NavigationComponents {
        stack,
        left_btn_stack,
        right_btn_box,
        nav_history,
    };

    // Group sorting-related components (orders, ascending flags)
    let sorting_components = SortingComponents {
        orders: sort_orders,
        ascending: sort_ascending,
        ascending_artists: sort_ascending_artists,
    };

    // Group display settings (screen info, badges, year format, zoom level)
    let display_settings = DisplaySettings {
        screen_info,
        show_dr_badges,
        use_original_year,
        current_zoom_level,
    };

    // Create the RefreshService instance
    let service = Rc::new(RefreshService::new(
        db_pool,
        albums_components,
        artists_components,
        navigation_components,
        sorting_components,
        display_settings,
        sender.clone(),
        player_bar,
        window,
    ));

    // Create the refresh UI closure from the service
    let refresh_library_ui = service.clone().create_refresh_closure();
    (sender, receiver, refresh_library_ui, service)
}

/// Sets up a periodic refresh of the library UI when the monitor geometry changes.
/// This ensures that the UI adapts to screen size changes by recalculating cover and tile sizes.
///
/// # Arguments
/// * `refresh_service` - An `Rc` wrapped `RefreshService` instance containing the shared state.
/// * `screen_width` - The initial width of the primary screen.
/// * `is_settings_open` - A `Rc<Cell<bool>>` indicating whether the settings dialog is currently open.
///   The UI will not refresh if settings are open to prevent visual glitches.
pub fn setup_live_monitor_refresh(
    refresh_service: Rc<RefreshService>,
    screen_info: Rc<RefCell<ScreenInfo>>,
    is_settings_open: Rc<Cell<bool>>,
    current_zoom_level: Option<Rc<Cell<ZoomLevel>>>,
) {
    let is_settings_open_cloned = is_settings_open.clone();

    // Increase the interval to reduce CPU usage and prevent excessive refreshing
    // Changed from 10 seconds to 60 seconds to significantly reduce resource usage
    timeout_add_local(Duration::from_secs(60), move || {
        // Add diagnostic logging
        println!("Live monitor refresh check triggered");
        if !is_settings_open_cloned.get() {
            let new_screen_info = ScreenInfo::new();
            if new_screen_info.width != screen_info.borrow().width {
                println!(
                    "Screen width changed from {} to {}, triggering refresh",
                    screen_info.borrow().width,
                    new_screen_info.width
                );
                *screen_info.borrow_mut() = new_screen_info;

                // Apply zoom level if available
                if let Some(zoom_level) = &current_zoom_level {
                    let zoom = zoom_level.get();
                    let cover_size = zoom.cover_size();
                    let tile_size = zoom.tile_size();
                    screen_info
                        .borrow_mut()
                        .update_with_zoom(cover_size, tile_size);
                }

                // Refresh the library UI with current sort orders when screen geometry changes
                (refresh_service.clone().create_refresh_closure())(
                    refresh_service.sort_ascending.get(),
                    refresh_service.sort_ascending_artists.get(),
                );
            }
        }
        Continue
    });
}

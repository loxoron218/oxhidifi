use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use glib::{ControlFlow::Continue, MainContext, source::timeout_add_local};
use gtk4::{FlowBox, Label, Stack};
use libadwaita::{ApplicationWindow, Clamp, ViewStack, prelude::WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    ui::{
        components::sorting::sorting_types::SortOrder,
        grids::album_grid_population::populate_albums_grid,
        grids::artist_grid_population::populate_artist_grid, search::clear_grid,
    },
    utils::screen::ScreenInfo,
};

/// A service struct that encapsulates all the shared state and logic required for refreshing
/// the library UI. This centralizes the management of UI components and data, simplifying
/// function signatures and improving maintainability.
pub struct RefreshService {
    db_pool: Arc<SqlitePool>,
    albums_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    artist_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    artists_stack_cell: Rc<RefCell<Option<Stack>>>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    stack: Rc<ViewStack>, // Main content stack
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Clamp,
    screen_info: Rc<RefCell<ScreenInfo>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    window: ApplicationWindow,
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    album_count_label: Rc<Label>,
    artist_count_label: Rc<Label>,
    header_btn_stack: Rc<ViewStack>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>, // Channel sender for scan feedback
}

impl RefreshService {
    /// Creates a new `RefreshService` instance, initializing it with all necessary UI components
    /// and shared data.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
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
        window: ApplicationWindow,
        scanning_label_albums: Label,
        scanning_label_artists: Label,
        album_count_label: Rc<Label>,
        artist_count_label: Rc<Label>,
        header_btn_stack: Rc<ViewStack>,
        nav_history: Rc<RefCell<Vec<String>>>,
        sender: UnboundedSender<()>,
    ) -> Self {
        Self {
            db_pool,
            albums_grid_cell,
            albums_stack_cell,
            artist_grid_cell,
            artists_stack_cell,
            sort_orders,
            stack,
            left_btn_stack,
            right_btn_box,
            screen_info,
            sort_ascending,
            sort_ascending_artists,
            window,
            scanning_label_albums,
            scanning_label_artists,
            album_count_label,
            artist_count_label,
            header_btn_stack,
            nav_history,
            sender,
        }
    }

    /// Sets the visible child name for a given inner stack (e.g., albums or artists) based on
    /// whether scanning is active.
    fn set_inner_stack_state(&self, inner_stack: &Stack, is_scanning_visible: bool) {
        if is_scanning_visible {
            inner_stack.set_visible_child_name("scanning_state");
        } else {
            inner_stack.set_visible_child_name("loading_state");
        }
    }

    /// Returns a `Rc<dyn Fn(bool, bool)>` closure that, when called, refreshes the library UI
    /// for either albums or artists based on the currently visible tab.
    /// This is the core refresh logic used throughout the application.
    pub fn create_refresh_closure(self: Rc<Self>) -> Rc<dyn Fn(bool, bool)> {
        Rc::new(
            move |sort_ascending_param: bool, sort_ascending_artists_param: bool| {
                // Update the sort direction cells
                self.sort_ascending.set(sort_ascending_param);
                self.sort_ascending_artists
                    .set(sort_ascending_artists_param);

                // Clone `self` for the async block to ensure ownership is transferred correctly
                let service_clone = Rc::clone(&self);

                MainContext::default().spawn_local(async move {
                    let current_tab = service_clone
                        .stack
                        .visible_child_name()
                        .unwrap_or_else(|| "albums".into());

                    if current_tab == "albums" {
                        if let (Some(albums_grid), Some(albums_inner_stack)) = (
                            service_clone.albums_grid_cell.borrow().as_ref(),
                            service_clone.albums_stack_cell.borrow().as_ref(),
                        ) {
                            clear_grid(albums_grid);
                            service_clone.set_inner_stack_state(
                                albums_inner_stack,
                                service_clone.scanning_label_albums.is_visible(),
                            );
                            populate_albums_grid(
                                albums_grid,
                                service_clone.db_pool.clone(),
                                service_clone.sort_ascending.get(),
                                Rc::clone(&service_clone.sort_orders),
                                &service_clone.screen_info,
                                &service_clone.scanning_label_albums,
                                service_clone.sender.clone(),
                                &service_clone.stack,
                                &service_clone.header_btn_stack,
                                &service_clone.right_btn_box,
                                albums_inner_stack,
                                &service_clone.album_count_label,
                            )
                            .await;
                        }
                    } else if current_tab == "artists" {
                        if let (Some(artist_grid), Some(artists_inner_stack)) = (
                            service_clone.artist_grid_cell.borrow().as_ref(),
                            service_clone.artists_stack_cell.borrow().as_ref(),
                        ) {
                            clear_grid(artist_grid);
                            service_clone.set_inner_stack_state(
                                artists_inner_stack,
                                service_clone.scanning_label_artists.is_visible(),
                            );
                            populate_artist_grid(
                                artist_grid,
                                service_clone.db_pool.clone(),
                                service_clone.sort_ascending_artists.get(),
                                &service_clone.stack,
                                &service_clone.left_btn_stack,
                                &service_clone.right_btn_box,
                                &service_clone.window,
                                &service_clone.scanning_label_artists,
                                service_clone.sender.clone(),
                                service_clone.nav_history.clone(),
                                artists_inner_stack,
                                service_clone.artist_count_label.clone(),
                            );
                        }
                    }
                });
            },
        )
    }
}

/// Sets up the library refresh channel and the refresh UI closure.
/// This function is the primary entry point for initializing the refresh mechanism.
///
/// Returns a tuple containing:
/// - `UnboundedSender<()>`: A sender to trigger UI refreshes from other parts of the application.
/// - `UnboundedReceiver<()>`: A receiver for the refresh signals.
/// - `Rc<dyn Fn(bool, bool)>`: A refresh closure that can be called to explicitly refresh the UI.
#[allow(clippy::too_many_arguments)]
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
    window: ApplicationWindow,
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    album_count_label: Rc<Label>,
    artist_count_label: Rc<Label>,
    header_btn_stack: Rc<ViewStack>,
    nav_history: Rc<RefCell<Vec<String>>>,
) -> (
    UnboundedSender<()>,
    UnboundedReceiver<()>,
    Rc<dyn Fn(bool, bool)>,
    Rc<RefreshService>, // Also return the service instance
) {
    let (sender, receiver) = unbounded_channel::<()>();

    // Create the RefreshService instance
    let service = Rc::new(RefreshService::new(
        db_pool,
        albums_grid_cell,
        albums_stack_cell,
        artist_grid_cell,
        artists_stack_cell,
        sort_orders,
        stack.clone(),
        left_btn_stack,
        right_btn_box,
        screen_info,
        sort_ascending,
        sort_ascending_artists,
        window,
        scanning_label_albums,
        scanning_label_artists,
        album_count_label,
        artist_count_label,
        header_btn_stack,
        nav_history,
        sender.clone(),
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
///                        The UI will not refresh if settings are open to prevent visual glitches.
pub fn setup_live_monitor_refresh(
    refresh_service: Rc<RefreshService>,
    screen_info: Rc<RefCell<ScreenInfo>>,
    is_settings_open: Rc<Cell<bool>>,
) {
    let is_settings_open_cloned = is_settings_open.clone();
    timeout_add_local(Duration::from_secs(1), move || {
        if !is_settings_open_cloned.get() {
            let new_screen_info = ScreenInfo::new();
            if new_screen_info.width != screen_info.borrow().width {
                *screen_info.borrow_mut() = new_screen_info;
                (refresh_service.clone().create_refresh_closure())(
                    refresh_service.sort_ascending.get(),
                    refresh_service.sort_ascending_artists.get(),
                );
            }
        }
        Continue
    });
}

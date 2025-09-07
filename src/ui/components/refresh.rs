use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use glib::{
    ControlFlow::Continue, MainContext, WeakRef, clone::Downgrade, source::timeout_add_local,
};
use gtk4::{ColumnView, FlowBox, Label, Stack, Window, gio::ListStore};
use libadwaita::{Clamp, ViewStack, prelude::WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    ui::{
        components::{
            player_bar::PlayerBar,
            view_controls::{
                list_view::population::populate_albums_column_view,
                sorting_controls::types::SortOrder, view_mode::ViewMode::ListView,
            },
        },
        grids::{
            album_grid_population::populate_albums_grid,
            album_grid_rebuilder::rebuild_albums_grid_for_window,
            album_grid_state::AlbumGridState::Empty, artist_grid_population::populate_artist_grid,
        },
        search::clear_grid,
    },
    utils::screen::ScreenInfo,
};

/// A service struct that encapsulates all the shared state and logic required for refreshing
/// the library UI. This centralizes the management of UI components and data, simplifying
/// function signatures and improving maintainability.
#[derive(Clone)]
pub struct RefreshService {
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
    sender: UnboundedSender<()>,
    pub show_dr_badges: Rc<Cell<bool>>,
    pub use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    column_view_model: Rc<RefCell<Option<ListStore>>>,
    pub column_view_widget: Rc<RefCell<Option<ColumnView>>>,
    previous_show_dr_badges: Cell<bool>,
    window: Window,
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
        scanning_label_albums: Label,
        scanning_label_artists: Label,
        album_count_label: Rc<Label>,
        artist_count_label: Rc<Label>,
        nav_history: Rc<RefCell<Vec<String>>>,
        sender: UnboundedSender<()>,
        show_dr_badges: Rc<Cell<bool>>,
        use_original_year: Rc<Cell<bool>>,
        player_bar: PlayerBar,
        window: Window,
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
            scanning_label_albums,
            scanning_label_artists,
            album_count_label,
            artist_count_label,
            nav_history,
            sender,
            show_dr_badges: show_dr_badges.clone(),
            use_original_year,
            player_bar,
            column_view_model: Rc::new(RefCell::new(None)),
            column_view_widget: Rc::new(RefCell::new(None)),
            previous_show_dr_badges: Cell::new(show_dr_badges.get()),
            window,
        }
    }

    /// Sets the visible child name for a given inner stack (e.g., albums or artists) based on
    /// whether scanning is active.
    fn set_inner_stack_state(&self, inner_stack: &Stack, is_scanning_visible: bool) {
        let child_name = if is_scanning_visible {
            "scanning_state"
        } else {
            "loading_state"
        };
        inner_stack.set_visible_child_name(child_name);
    }

    /// Sets the ColumnView model reference for ListView mode
    pub fn set_column_view_model(&self, model: Option<ListStore>) {
        *self.column_view_model.borrow_mut() = model;
    }

    /// Sets the ColumnView widget reference for ListView mode
    pub fn set_column_view_widget(&self, widget: Option<ColumnView>) {
        *self.column_view_widget.borrow_mut() = widget;
    }

    /// Gets a weak reference to the left button stack
    pub fn get_left_btn_stack(&self) -> WeakRef<ViewStack> {
        // Borrow the ViewStack from the Rc and then downgrade it
        // This should give us a glib::WeakRef<ViewStack> instead of std::rc::Weak<ViewStack>
        self.left_btn_stack.as_ref().downgrade()
    }

    /// Gets a clone of the player bar
    pub fn get_player_bar(&self) -> PlayerBar {
        self.player_bar.clone()
    }

    /// A new helper function specifically for the albums tab
    async fn repopulate_albums_tab(&self) {
        if let (Some(grid), Some(stack)) = (
            self.albums_grid_cell.borrow().as_ref(),
            self.albums_stack_cell.borrow().as_ref(),
        ) {
            clear_grid(grid);
            self.set_inner_stack_state(stack, self.scanning_label_albums.is_visible());
            populate_albums_grid(
                grid,
                self.db_pool.clone(),
                self.sort_ascending.get(),
                Rc::clone(&self.sort_orders),
                &self.screen_info,
                stack,
                &self.album_count_label,
                self.show_dr_badges.clone(),
                self.use_original_year.clone(),
                self.player_bar.clone(),
            )
            .await;
        } else {
            // If we don't have the grid or stack, but we have a stack, set it to a default state
            // to avoid leaving the UI in a loading state indefinitely
            if let Some(stack) = self.albums_stack_cell.borrow().as_ref() {
                // Set to empty state as a fallback
                stack.set_visible_child_name(Empty.as_str());
            }
        }
    }

    /// A new helper function specifically for the artists tab
    async fn repopulate_artists_tab(&self) {
        if let (Some(grid), Some(stack)) = (
            self.artist_grid_cell.borrow().as_ref(),
            self.artists_stack_cell.borrow().as_ref(),
        ) {
            clear_grid(grid);
            self.set_inner_stack_state(stack, self.scanning_label_artists.is_visible());
            populate_artist_grid(
                grid,
                self.db_pool.clone(),
                self.sort_ascending_artists.get(),
                &self.stack,
                &self.left_btn_stack,
                &self.right_btn_box,
                &self.screen_info,
                self.sender.clone(),
                self.nav_history.clone(),
                stack,
                self.artist_count_label.clone(),
                self.show_dr_badges.clone(),
                self.use_original_year.clone(),
                self.player_bar.clone(),
            );
        }
    }

    /// A new helper function specifically for repopulating the ColumnView in ListView mode
    async fn repopulate_column_view(&self, window: &Window) {
        // Get the scanning label from the albums stack if it exists
        let scanning_label = if self.albums_stack_cell.borrow().as_ref().is_some() {
            // Check if scanning label is visible
            self.scanning_label_albums.is_visible()
        } else {
            false
        };

        // Check if the "Show DR Value Badges" setting has changed
        let current_show_dr_badges = self.show_dr_badges.get();
        let previous_show_dr_badges = self.previous_show_dr_badges.get();

        // Check if the "Use Original Year" setting has changed
        // We need to track the previous state of this setting as well
        thread_local! {
            // Default to true to force initial population
            static PREVIOUS_USE_ORIGINAL_YEAR: Cell<bool> = Cell::new(true);
        }
        let current_use_original_year = self.use_original_year.get();
        let previous_use_original_year = PREVIOUS_USE_ORIGINAL_YEAR.with(|cell| cell.get());
        if current_show_dr_badges != previous_show_dr_badges
            || current_use_original_year != previous_use_original_year
        {
            // Update the previous states
            self.previous_show_dr_badges.set(current_show_dr_badges);
            PREVIOUS_USE_ORIGINAL_YEAR.with(|cell| cell.set(current_use_original_year));

            // Rebuild the albums grid with ListView mode
            let model = rebuild_albums_grid_for_window(
                &self.stack,
                &self.scanning_label_albums,
                &self.screen_info,
                &self.albums_grid_cell,
                &self.albums_stack_cell,
                window,
                &self.db_pool,
                &self.sender,
                self.album_count_label.clone(),
                ListView,
                current_use_original_year,
                self.show_dr_badges.clone(),
                Some(Rc::new(self.clone())),
            );

            // Set the ColumnView model in the RefreshService
            self.set_column_view_model(model.clone());

            // If we have a model, populate the column view with data
            if let Some(model) = model {
                // Get the albums stack to pass to the population function
                if let Some(albums_stack) = self.albums_stack_cell.borrow().as_ref() {
                    let albums_stack_clone = albums_stack.clone();

                    // Set the inner stack state based on scanning visibility
                    self.set_inner_stack_state(&albums_stack_clone, scanning_label);

                    // Repopulate the ColumnView with updated data
                    populate_albums_column_view(
                        &model,
                        self.db_pool.clone(),
                        self.sort_ascending.get(),
                        Rc::clone(&self.sort_orders),
                        &albums_stack_clone,
                        &self.album_count_label,
                        self.use_original_year.clone(),
                        self.player_bar.clone(),
                    )
                    .await;
                }
            }
        } else {
            // For refresh operations, we should use the existing model, not rebuild the grid
            if let Some(model) = self.column_view_model.borrow().as_ref() {
                // Get the albums stack to pass to the population function
                if let Some(albums_stack) = self.albums_stack_cell.borrow().as_ref() {
                    let albums_stack_clone = albums_stack.clone();

                    // Set the inner stack state based on scanning visibility
                    self.set_inner_stack_state(&albums_stack_clone, scanning_label);

                    // Repopulate the ColumnView with updated data
                    populate_albums_column_view(
                        model,
                        self.db_pool.clone(),
                        self.sort_ascending.get(),
                        Rc::clone(&self.sort_orders),
                        &albums_stack_clone,
                        &self.album_count_label,
                        self.use_original_year.clone(),
                        self.player_bar.clone(),
                    )
                    .await;
                } else {
                }
            } else {
                // If we don't have a model but we have a stack, set it to a default state
                // to avoid leaving the UI in a loading state indefinitely
                if let Some(stack) = self.albums_stack_cell.borrow().as_ref() {
                    // Set to empty state as a fallback
                    stack.set_visible_child_name(Empty.as_str());
                }
            }
        }
    }
    /// Returns a `Rc<dyn Fn(bool, bool)>` closure that, when called, refreshes the library UI
    /// for either albums or artists based on the currently visible tab.
    /// This is the core refresh logic used throughout the application.
    pub fn create_refresh_closure(self: Rc<Self>) -> Rc<dyn Fn(bool, bool)> {
        Rc::new(
            move |sort_ascending_param: bool, sort_ascending_artists_param: bool| {
                // Update the sort direction cells based on the current tab
                let current_tab = self
                    .stack
                    .visible_child_name()
                    .unwrap_or_else(|| "albums".into());
                match current_tab.as_str() {
                    "albums" => {
                        self.sort_ascending.set(sort_ascending_param);
                    }
                    "artists" => {
                        self.sort_ascending_artists
                            .set(sort_ascending_artists_param);
                    }

                    // This arm explicitly handles any other tab values by doing nothing.
                    _ => {}
                }

                // Clone `self` for the async block to ensure ownership is transferred correctly
                let service_clone = Rc::clone(&self);
                MainContext::default().spawn_local(async move {
                    let current_tab = service_clone
                        .stack
                        .visible_child_name()
                        .unwrap_or_else(|| "albums".into());

                    // The main logic is now clean, readable, and easy to extend.
                    match current_tab.as_str() {
                        "albums" => {
                            // Check if we're in GridView mode by checking if we have an albums grid cell
                            let is_grid_view = service_clone.albums_grid_cell.borrow().is_some();
                            if is_grid_view {
                                // Repopulate the albums grid (GridView mode)
                                service_clone.repopulate_albums_tab().await;
                            } else {
                                // Repopulate the ColumnView with updated data (ListView mode)
                                service_clone
                                    .repopulate_column_view(&service_clone.window)
                                    .await;
                            }
                        }
                        "artists" => service_clone.repopulate_artists_tab().await,

                        // Handle other tabs or do nothing
                        _ => {}
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
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    album_count_label: Rc<Label>,
    artist_count_label: Rc<Label>,
    nav_history: Rc<RefCell<Vec<String>>>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    window: Window,
) -> (
    UnboundedSender<()>,
    UnboundedReceiver<()>,
    Rc<dyn Fn(bool, bool)>,
    Rc<RefreshService>,
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
        scanning_label_albums,
        scanning_label_artists,
        album_count_label,
        artist_count_label,
        nav_history,
        sender.clone(),
        show_dr_badges,
        use_original_year,
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
///                        The UI will not refresh if settings are open to prevent visual glitches.
pub fn setup_live_monitor_refresh(
    refresh_service: Rc<RefreshService>,
    screen_info: Rc<RefCell<ScreenInfo>>,
    is_settings_open: Rc<Cell<bool>>,
) {
    let is_settings_open_cloned = is_settings_open.clone();
    timeout_add_local(Duration::from_secs(3), move || {
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

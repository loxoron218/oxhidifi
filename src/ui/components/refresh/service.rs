use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    ColumnView, FlowBox, Label, Stack, Window,
    gio::ListStore,
    glib::{MainContext, WeakRef, clone::Downgrade},
};
use libadwaita::{Clamp, ViewStack};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    ui::components::{
        player_bar::PlayerBar,
        view_controls::{ZoomLevel, sorting_controls::types::SortOrder},
    },
    utils::screen::ScreenInfo,
};

/// A service struct that encapsulates all the shared state and logic required for refreshing
/// the library UI. This centralizes the management of UI components and data, simplifying
/// function signatures and improving maintainability.
#[derive(Clone)]
pub struct RefreshService {
    pub db_pool: Arc<SqlitePool>,
    pub albums_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    pub albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    pub artist_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    pub artists_stack_cell: Rc<RefCell<Option<Stack>>>,
    pub sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    pub stack: Rc<ViewStack>,
    pub left_btn_stack: Rc<ViewStack>,
    pub right_btn_box: Clamp,
    pub screen_info: Rc<RefCell<ScreenInfo>>,
    pub sort_ascending: Rc<Cell<bool>>,
    pub sort_ascending_artists: Rc<Cell<bool>>,
    pub scanning_label_albums: Label,
    pub scanning_label_artists: Label,
    pub album_count_label: Rc<Label>,
    pub artist_count_label: Rc<Label>,
    pub nav_history: Rc<RefCell<Vec<String>>>,
    pub sender: UnboundedSender<()>,
    pub show_dr_badges: Rc<Cell<bool>>,
    pub use_original_year: Rc<Cell<bool>>,
    pub player_bar: PlayerBar,
    pub column_view_model: Rc<RefCell<Option<ListStore>>>,
    pub column_view_widget: Rc<RefCell<Option<ColumnView>>>,
    pub previous_show_dr_badges: Cell<bool>,
    pub window: Window,
    pub current_zoom_level: Option<Rc<Cell<ZoomLevel>>>,
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
        current_zoom_level: Option<Rc<Cell<ZoomLevel>>>,
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
            current_zoom_level,
        }
    }

    /// Sets the visible child name for a given inner stack (e.g., albums or artists) based on
    /// whether scanning is active.
    pub fn set_inner_stack_state(&self, inner_stack: &Stack, is_scanning_visible: bool) {
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

    /// Gets a clone of the column view model
    pub fn get_column_view_model(&self) -> Option<ListStore> {
        self.column_view_model.borrow().as_ref().cloned()
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

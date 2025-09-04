use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::{ui::components::view_controls::sorting_controls::types, utils::screen::ScreenInfo};

/// `WindowSharedState` struct encapsulates all the `Rc<Cell<T>>` and `Rc<RefCell<T>>` managed
/// shared state that is passed around and mutated by different parts of the UI logic.
///
/// This struct centralizes the management of application state, improving clarity and
/// reducing the number of individual `Rc` and `RefCell` clones in function signatures.
/// It promotes a more organized and maintainable way of handling application-wide state.
pub struct WindowSharedState {
    /// Stores the current sort orders for albums and artists.
    /// `RefCell` allows for mutable access to the `Vec<SortOrder>` within an `Rc`.
    pub sort_orders: Rc<RefCell<Vec<types::SortOrder>>>,
    /// Indicates whether albums should be sorted in ascending order.
    /// `Cell` is used for simple copyable types like `bool` to allow mutable interior access.
    pub sort_ascending: Rc<Cell<bool>>,
    /// Indicates whether artists should be sorted in ascending order.
    pub sort_ascending_artists: Rc<Cell<bool>>,
    /// Stores the name of the last active main tab ("albums" or "artists").
    /// Used for navigation and to restore context after returning from detail views.
    pub last_tab: Rc<Cell<&'static str>>,
    /// Stores the navigation history for the `ViewStack`.
    /// `RefCell<Vec<String>>` allows for adding/removing page names during navigation.
    pub nav_history: Rc<RefCell<Vec<String>>>,
    /// Stores the calculated cover art size for live updates.
    /// This size can change if the window is resized or moved to a different monitor.
    pub screen_info: Rc<RefCell<ScreenInfo>>,
    /// Flag to indicate if the settings dialog is currently open.
    /// Used to prevent unnecessary UI refreshes while a modal dialog is active.
    pub is_settings_open: Rc<Cell<bool>>,
    /// Indicates whether DR Value badges should be displayed.
    pub show_dr_badges: Rc<Cell<bool>>,
    /// Indicates whether the original release year should be used for display.
    pub use_original_year: Rc<Cell<bool>>,
    /// Indicates the preferred view mode for albums and artists (e.g., "Grid View", "List View").
    pub view_mode: Rc<RefCell<String>>,
}

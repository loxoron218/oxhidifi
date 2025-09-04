use std::{cell::RefCell, rc::Rc};

use gtk4::{Button, FlowBox, Label, Stack, ToggleButton};
use libadwaita::{ApplicationWindow, Clamp, ViewStack};

use crate::ui::{
    components::{player_bar::PlayerBar, view_controls::view_control_button::ViewControlButton},
    header::AppHeaderBar,
};

/// `WindowWidgets` struct encapsulates references to all the essential GTK widgets
/// that make up the main application window's user interface.
///
/// This struct is designed to be created once and then passed by reference (`&`)
/// or cloned (`.clone()`) as `Rc` or `Arc` to various UI functions that need to
/// interact with these widgets. It centralizes access to the UI components,
/// reducing argument clutter in function signatures and improving code readability.
///
/// Fields are public to allow direct access from UI-building and event-handling functions.
pub struct WindowWidgets {
    /// The main application window.
    pub window: ApplicationWindow,
    /// The main content `ViewStack` for navigating between different views (albums, artists, detail pages).
    pub stack: ViewStack,
    /// The `ViewStack` managing the left-aligned buttons in the header (e.g., add or back button).
    pub left_btn_stack: ViewStack,
    /// The `Clamp` widget containing the right-aligned utility buttons (search, sort, settings).
    pub right_btn_box: Clamp,
    /// The '+' button, typically used to add new content (e.g., music folders).
    pub add_button: Button,
    /// The back button, used for navigating back to previous views.
    pub back_button: Button,
    /// The settings button, which opens the application's configuration dialog.
    pub settings_button: Button,
    /// The search bar component, including its entry field, revealer, and trigger button.
    pub search_bar: AppHeaderBar,
    /// The sort button, used to change the sorting order of content.
    pub sort_button: Button,
    /// The view control button, used to change the view mode and access view options.
    pub view_control_button: ViewControlButton,
    /// The "Albums" toggle button in the tab bar.
    pub albums_btn: ToggleButton,
    /// The "Artists" toggle button in the tab bar.
    pub artists_btn: ToggleButton,
    /// Label indicating scanning progress for albums.
    pub scanning_label_albums: Label,
    /// Label indicating scanning progress for artists.
    pub scanning_label_artists: Label,
    /// Label for displaying the count of currently displayed albums.
    pub album_count_label: Rc<Label>,
    /// Label for displaying the count of currently displayed artists.
    pub artist_count_label: Rc<Label>,
    /// `Rc<RefCell<Option<FlowBox>>>` holding the albums grid for dynamic updates.
    /// This pattern allows for safe, mutable interior access to the grid from multiple
    /// parts of the application, particularly when the grid needs to be rebuilt or cleared.
    pub albums_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    /// `Rc<RefCell<Option<Stack>>>` holding the albums inner stack for dynamic updates.
    /// Used to switch between loading, empty, and populated states for the albums view.
    pub albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    /// `Rc<RefCell<Option<FlowBox>>>` holding the artists grid for dynamic updates.
    pub artist_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    /// `Rc<RefCell<Option<Stack>>>` holding the artists inner stack for dynamic updates.
    /// Used to switch between loading, empty, and populated states for the artists view.
    pub artists_stack_cell: Rc<RefCell<Option<Stack>>>,
    /// The player bar widget, displayed at the bottom of the window when a song is playing.
    pub player_bar: PlayerBar,
}

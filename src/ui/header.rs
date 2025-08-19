use gtk4::{
    Align::End, Box, Button, HeaderBar, Image, Label, Orientation::Horizontal, ToggleButton, Widget,
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, ButtonExt, IsA, WidgetExt},
};

use crate::ui::search_bar::SearchBar;

/// Helper to create a GTK Button with a specified icon name.
fn create_icon_button(icon_name: &str) -> Button {
    Button::builder().icon_name(icon_name).build()
}

/// Helper to create a horizontal Box and wrap it in a Clamp for ViewStack children.
/// This pattern is common for adding widgets to a ViewStack, ensuring proper layout.
fn create_view_stack_child_box(widget: &impl IsA<Widget>) -> Clamp {
    let inner_box = Box::builder().orientation(Horizontal).build();
    inner_box.append(widget);
    Clamp::builder().child(&inner_box).build()
}

/// Helper function to create a toggle button with an icon and label.
/// This pattern is used for the "Albums" and "Artists" tabs.
fn create_tab_toggle_button(icon_name: &str, label_text: &str, is_active: bool) -> ToggleButton {
    let button = ToggleButton::builder().active(is_active).build();
    button.set_has_frame(false); // Remove the default button frame for a cleaner look.

    let content_box = Box::builder()
        .orientation(Horizontal)
        .spacing(4) // Small spacing between icon and label.
        .build();
    content_box.append(&Image::from_icon_name(icon_name));
    content_box.append(&Label::builder().label(label_text).build());
    button.set_child(Some(&content_box));
    button
}

/// Struct to hold header widgets and state.
///
/// This struct encapsulates all the interactive GTK widgets that make up the
/// application's header bar. It allows for easy access and manipulation of these
/// components from other parts of the application, particularly from `window.rs`
/// for connecting signals and managing UI state.
#[derive(Clone)]
pub struct AppHeaderBar {
    /// The `ViewStack` managing the left-aligned buttons in the header (e.g., add or back button).
    /// This allows for animated transitions between different button states.
    pub left_btn_stack: ViewStack,

    /// The `Clamp` widget containing the right-aligned utility buttons (search, sort, settings).
    /// `Clamp` is used here for responsive layout and alignment.
    pub right_btn_box: Clamp,

    /// The '+' button, typically used to add new content (e.g., music folders).
    pub add_button: Button,

    /// The back button, used for navigating back to previous views (e.g., from an album detail page).
    pub back_button: Button,

    /// The settings button, which opens the application's configuration dialog.
    pub settings_button: Button,

    /// The search bar component, including its entry field, revealer, and trigger button.
    pub search_bar: SearchBar,

    /// The sort button, used to change the sorting order of content in the main views.
    pub sort_button: Button,
}

/// Builds the application's primary header bar, including left-aligned action buttons,
/// the search bar, and right-aligned utility buttons.
///
/// This function constructs and arranges all the necessary GTK widgets that comprise
/// the `AppHeaderBar` struct, which is then used in the main application window.
///
/// # Returns
/// An `AppHeaderBar` instance containing all the constructed header widgets.
pub fn build_header_bar() -> AppHeaderBar {
    // Create individual buttons using the helper function for consistency and brevity.
    let add_button = create_icon_button("list-add");
    let search_bar = SearchBar::new();
    let settings_button = create_icon_button("open-menu-symbolic");
    let back_button = create_icon_button("go-previous-symbolic");
    let sort_button = create_icon_button("view-sort-descending-symbolic");

    // Left-aligned button stack for animated transitions (e.g., main menu vs. back button).
    let left_btn_stack = ViewStack::builder().build();

    // Add button: Used for actions like adding new music folders.
    // Wrapped in a Clamp and added to the ViewStack for animated visibility.
    left_btn_stack.add_titled(
        &create_view_stack_child_box(&add_button),
        Some("main"),
        "Main",
    );

    // Back button: Appears when navigating into detail views (e.g., album page).
    // Wrapped in a Clamp and added to the ViewStack.
    left_btn_stack.add_titled(
        &create_view_stack_child_box(&back_button),
        Some("back"),
        "Back",
    );
    left_btn_stack.set_visible_child_name("main"); // Initially show the main (add) button.

    // Right-aligned box containing search, sort, and settings buttons.
    let right_btn_inner = Box::builder()
        .orientation(Horizontal)
        .spacing(6) // Standard spacing between header elements.
        .build();
    right_btn_inner.append(&search_bar.button); // The search icon button.
    right_btn_inner.append(&*search_bar.revealer); // The animated search entry revealer.
    right_btn_inner.append(&sort_button); // Button to change sorting order.
    right_btn_inner.append(&settings_button); // Button to open application settings.

    let right_btn_box = Clamp::builder()
        .child(&right_btn_inner)
        .halign(End) // Align to the end (right) of the header bar.
        .build();
    right_btn_box.set_visible(true); // Ensure visibility by default.

    AppHeaderBar {
        left_btn_stack,
        right_btn_box,
        add_button,
        back_button,
        settings_button,
        search_bar,
        sort_button,
    }
}

/// Utility function to construct and configure the main `gtk4::HeaderBar`.
///
/// This function takes the pre-built left, right, and center widgets and composes them
/// into a single `HeaderBar` instance, which serves as the top-level header for the application window.
///
/// # Arguments
/// * `left_btn_stack` - The `ViewStack` containing left-aligned buttons (e.g., add, back).
/// * `right_btn_box` - The `Clamp` widget containing right-aligned utility buttons (e.g., search, settings).
/// * `center_box` - The `Clamp` widget containing the central title widget (e.g., tab bar).
///
/// # Returns
/// A fully configured `gtk4::HeaderBar` ready to be added to the application window.
pub fn build_main_headerbar(
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    center_box: &Clamp,
) -> HeaderBar {
    let header_bar = HeaderBar::builder().build();
    header_bar.pack_start(left_btn_stack); // Place left-aligned widgets.
    header_bar.set_title_widget(Some(center_box)); // Set the central title widget.
    header_bar.pack_end(right_btn_box); // Place right-aligned widgets.
    header_bar
}

/// Builds the Albums/Artists tab bar, which serves as the central title widget in the header.
///
/// This function creates two toggle buttons, one for "Albums" and one for "Artists",
/// each with an icon and a label. These buttons allow the user to switch between
/// different views of their music library.
///
/// # Returns
/// A tuple containing:
/// 1. A `gtk4::Box` holding both toggle buttons, configured as the tab bar.
/// 2. The "Albums" `ToggleButton` instance.
/// 3. The "Artists" `ToggleButton` instance.
pub fn build_tab_bar() -> (Box, ToggleButton, ToggleButton) {
    // Albums toggle button, active by default.
    let albums_btn = create_tab_toggle_button("folder-music-symbolic", "Albums", true);

    // Artists toggle button, inactive by default.
    let artists_btn = create_tab_toggle_button("avatar-default-symbolic", "Artists", false);

    // Container for the tab buttons.
    let tab_bar = Box::builder()
        .orientation(Horizontal)
        .spacing(6) // Spacing between the two tab buttons.
        .build();
    tab_bar.append(&albums_btn);
    tab_bar.append(&artists_btn);

    (tab_bar, albums_btn, artists_btn)
}

use gtk4::{Align, Box, Button, HeaderBar, Image, Label, Orientation, ToggleButton};
use libadwaita::{Clamp, ViewStack};
use libadwaita::prelude::{BoxExt, ButtonExt, WidgetExt};

use crate::ui::search_bar::SearchBar;

/// Struct to hold header widgets and state
#[derive(Clone)]
pub struct AppHeaderBar {

    /// Left button stack.
    pub left_btn_stack: ViewStack,

    /// Right button box.
    pub right_btn_box: Clamp,

    /// Add button.
    pub add_button: Button,

    /// Back button.
    pub back_button: Button,

    /// Settings button.
    pub settings_button: Button,

    /// Search bar.
    pub search_bar: SearchBar,

    /// Sort button.
    pub sort_button: Button,
}

/// Build the header bar (left and right button stacks, search bar, etc.)
pub fn build_header_bar() -> AppHeaderBar {

    // Create buttons
    let add_button = Button::builder()
        .icon_name("list-add")
        .build();
    let search_bar = SearchBar::new();
    let settings_button = Button::builder()
        .icon_name("open-menu-symbolic")
        .build();
    let back_button = Button::builder()
        .icon_name("go-previous-symbolic")
        .build();
    let sort_button = Button::builder()
        .icon_name("view-sort-descending-symbolic")
        .build();

    // Stack for left header button (animated)
    let left_btn_stack = ViewStack::builder()
        .build();

    // Box for '+' and rescan button
    let add_btn_inner = Box::builder()
        .orientation(Orientation::Horizontal)
        .build();
    add_btn_inner.append(&add_button);
    let add_btn_box = Clamp::builder()
        .child(&add_btn_inner)
        .build();
    left_btn_stack.add_titled(&add_btn_box, Some("main"), "Main");

    // Box for back button
    let back_btn_inner = Box::builder()
        .orientation(Orientation::Horizontal)
        .build();
    back_btn_inner.append(&back_button);
    let back_btn_box = Clamp::builder()
        .child(&back_btn_inner)
        .build();
    left_btn_stack.add_titled(&back_btn_box, Some("back"), "Back");
    left_btn_stack.set_visible_child_name("main");

    // Right-side box for search, sort, settings
    let right_btn_inner = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    right_btn_inner.append(&search_bar.button);
    right_btn_inner.append(&*search_bar.revealer);
    right_btn_inner.append(&sort_button);
    right_btn_inner.append(&settings_button);
    let right_btn_box = Clamp::builder()
        .child(&right_btn_inner)
        .halign(Align::End)
        .build();
    right_btn_box.set_visible(true);

    // Return a HeaderBar struct containing all header widgets for use in the main window
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

/// Utility to construct and configure the main GTK HeaderBar with left, center, and right widgets.
pub fn build_main_headerbar(
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    center_box: &Clamp,
) -> HeaderBar {
    let header_bar = HeaderBar::builder().build();
    header_bar.pack_start(left_btn_stack);
    header_bar.set_title_widget(Some(center_box));
    header_bar.pack_end(right_btn_box);
    header_bar
}

/// Build the Albums/Artists tab bar with toggle buttons
pub fn build_tab_bar() -> (Box, ToggleButton, ToggleButton) {

    // Albums toggle button
    let albums_btn = ToggleButton::builder()
        .active(true)
        .build();
    albums_btn.set_has_frame(false.into());
    let albums_box = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();
    let albums_icon = Image::from_icon_name("folder-music-symbolic");
    let albums_label = Label::builder()
        .label("Albums")
        .build();
    albums_box.append(&albums_icon);
    albums_box.append(&albums_label);
    albums_btn.set_child(Some(&albums_box));

    // Artists toggle button
    let artists_btn = ToggleButton::builder()
        .active(false.into())
        .build();
    artists_btn.set_has_frame(false.into());
    let artists_box = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();
    let artists_icon = Image::from_icon_name("avatar-default-symbolic");
    let artists_label = Label::builder()
        .label("Artists")
        .build();
    artists_box.append(&artists_icon);
    artists_box.append(&artists_label);
    artists_btn.set_child(Some(&artists_box));

    // Tab bar
    let tab_bar = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    tab_bar.append(&albums_btn);
    tab_bar.append(&artists_btn);
    (tab_bar, albums_btn, artists_btn)
}

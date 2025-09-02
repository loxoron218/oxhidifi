use std::{cell::RefCell, rc::Rc};

use gtk4::{
    Align::{Center, Fill, Start},
    Box, Button, FlowBox, Label,
    Orientation::Vertical,
    PolicyType::Automatic,
    ScrolledWindow,
    SelectionMode::None,
    Spinner, Stack, StackTransitionType,
};
use libadwaita::{
    StatusPage,
    prelude::{BoxExt, WidgetExt},
};

use crate::ui::{
    components::scan_feedback::create_scanning_label,
    grids::album_grid_state::AlbumGridState::{Empty, Loading, NoResults, Populated, Scanning},
};

/// Builds the main `gtk4::Stack` and `gtk4::FlowBox` for the albums grid.
///
/// This function constructs the UI elements for various states of the album display:
/// empty, no results, loading, scanning, and the populated grid itself.
/// It sets up the initial visibility to the loading state.
///
/// # Arguments
/// * `scanning_label` - A `gtk4::Label` to display scanning feedback.
/// * `cover_size` - The calculated size for album covers.
/// * `tile_size` - The calculated size for album tiles.
/// * `add_music_button` - A `gtk4::Button` to be displayed in the empty state.
///
/// # Returns
/// A tuple containing the `gtk4::Stack` managing the album views and the `gtk4::FlowBox`
/// where individual album tiles will be added.
pub fn build_albums_grid(
    scanning_label: &Label,
    _cover_size: i32,
    _tile_size: i32,
    add_music_button: &Button,
    album_count_label: Rc<Label>,
    _view_mode: Rc<RefCell<String>>,
) -> (Stack, FlowBox) {
    // --- Empty State (No Music Found) ---
    // This state is shown when the library is completely empty.
    add_music_button.add_css_class("suggested-action");
    let empty_state_status_page = StatusPage::builder()
        .icon_name("folder-music-symbolic")
        .title("No Music Found")
        .description("Add music to your library to get started.")
        .vexpand(true)
        .hexpand(true)
        .child(add_music_button)
        .build();

    let empty_state_container = Box::builder()
        .orientation(Vertical)
        .halign(Center)
        .valign(Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    empty_state_container.append(&empty_state_status_page);

    // --- No Results State (Search yielded no results) ---
    // This state is shown when a search query returns no matching albums.
    let no_results_status_page = StatusPage::builder()
        .icon_name("system-search-symbolic")
        .title("No Albums Found")
        .description("Try a different search query.")
        .vexpand(true)
        .hexpand(true)
        .build();
    let no_results_container = Box::builder()
        .orientation(Vertical)
        .halign(Center)
        .valign(Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    no_results_container.append(&no_results_status_page);

    // --- Loading State ---
    // Displayed while the application is fetching initial album data.
    let loading_spinner = Spinner::builder().spinning(true).build();
    loading_spinner.set_size_request(48, 48);
    let loading_state_container = Box::builder()
        .orientation(Vertical)
        .halign(Center)
        .valign(Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    loading_state_container.append(&loading_spinner);
    let scanning_label_widget = create_scanning_label();
    loading_state_container.append(&scanning_label_widget);
    scanning_label_widget.set_visible(true);

    // --- Scanning State ---
    // Displayed when the library is actively being scanned for new music.
    let scanning_spinner = Spinner::builder().spinning(true).build();
    scanning_spinner.set_size_request(48, 48);
    let scanning_state_container = Box::builder()
        .orientation(Vertical)
        .halign(Center)
        .valign(Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    scanning_state_container.append(&scanning_spinner);
    scanning_state_container.append(scanning_label);

    // --- Albums Grid (Populated State) ---
    // The actual FlowBox where album tiles will be rendered.
    let albums_grid = FlowBox::builder()
        .valign(Start)
        .max_children_per_line(128)
        .row_spacing(8)
        .column_spacing(8)
        .selection_mode(None)
        .homogeneous(true)
        .hexpand(true)
        .halign(Fill)
        .build();

    // Scrolled window to allow scrolling if content overflows.
    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(Automatic)
        .vscrollbar_policy(Automatic)
        .child(&albums_grid)
        .min_content_height(600)
        .min_content_width(410)
        .vexpand(true)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(24)
        .hexpand(true)
        .halign(Fill)
        .build();

    // --- Main Albums Stack ---
    // A Stack widget to manage the different states (loading, empty, populated, etc.).
    let albums_stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .build();

    // Add all state containers and the populated grid to the stack.
    albums_stack.add_named(&loading_state_container, Some(Loading.as_str()));
    albums_stack.add_named(&empty_state_container, Some(Empty.as_str()));
    albums_stack.add_named(&no_results_container, Some(NoResults.as_str()));
    albums_stack.add_named(&scanning_state_container, Some(Scanning.as_str()));

    // The actual populated grid is placed inside another Box for potential future additions
    // like a search bar or filters above the grid.
    let albums_content_box = Box::builder().orientation(Vertical).build();
    albums_content_box.prepend(&*album_count_label);

    // The album_count_label is now passed from main_window/builder.rs
    albums_content_box.append(&scrolled_window);
    albums_stack.add_named(&albums_content_box, Some(Populated.as_str()));

    // Set the initial visible child to the loading state.
    albums_stack.set_visible_child_name(Loading.as_str());
    (albums_stack, albums_grid)
}

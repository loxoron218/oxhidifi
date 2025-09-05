use std::rc::Rc;

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

use crate::ui::components::scan_feedback::create_scanning_label;

/// Builds the artists grid and its containing `Stack` for managing different states.
///
/// This function constructs the UI components necessary for displaying artists,
/// including empty states, loading states, and the populated grid itself. It returns
/// a `Stack` (which manages the different states) and the `FlowBox` (the actual grid
/// where artist tiles will be inserted).
///
/// # Arguments
///
/// * `scanning_label` - The `gtk4::Label` used to indicate scanning progress.
/// * `add_music_button` - A `gtk4::Button` to be displayed in the empty state, prompting
///   the user to add music.
///
/// # Returns
///
/// A tuple containing:
/// * `Stack`: The `gtk4::Stack` widget that manages the different display states
///   (loading, empty, no results, scanning, populated grid).
/// * `FlowBox`: The `gtk4::FlowBox` widget where individual artist tiles will be added.
pub fn build_artist_grid(
    scanning_label: &Label,
    add_music_button: &Button,
    artist_count_label: Rc<Label>,
) -> (Stack, FlowBox) {
    // --- Empty State ---
    // This state is shown when no artists are found in the library and no scan is in progress.
    let empty_state_status_page = StatusPage::builder()
        .icon_name("avatar-default-symbolic")
        .title("No Artists Found")
        .description("Add music to your library to get started.")
        .vexpand(true)
        .hexpand(true)
        .build();

    // The add_music_button is passed in from `main_window/handlers.rs` and styled here.
    add_music_button.add_css_class("suggested-action");

    // Set the button as the child of the StatusPage.
    empty_state_status_page.set_child(Some(add_music_button));
    let empty_state_container = Box::builder()
        .orientation(Vertical)
        .halign(Center)
        .valign(Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    empty_state_container.append(&empty_state_status_page);

    // --- No Results State ---
    // This state is shown when a search yields no results.
    let no_results_status_page = StatusPage::builder()
        .icon_name("system-search-symbolic")
        .title("No Artists Found")
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

    // --- Artists Grid (Populated State) ---
    // This is the actual `FlowBox` where artist tiles will be dynamically added.
    let artist_grid = FlowBox::builder()
        .valign(Start)
        .max_children_per_line(128)
        .row_spacing(8)
        .column_spacing(8)
        .selection_mode(None)
        .homogeneous(true)
        .hexpand(true)
        .halign(Fill)
        .build();

    // Wrap the `FlowBox` in a `ScrolledWindow` to enable scrolling if content overflows.
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(Automatic)
        .vscrollbar_policy(Automatic)
        .child(&artist_grid)
        .vexpand(true)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(24)
        .build();

    // Allow horizontal expansion.
    scrolled.set_hexpand(true);

    // Fill available horizontal space.
    scrolled.set_halign(Fill);

    // The main `Stack` to manage the different views (loading, empty, populated, etc.).
    let artists_stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .build();

    // --- Loading State ---
    // Shown while initial data is being fetched.
    let loading_spinner = Spinner::builder().spinning(true).build();

    // Set a fixed size for the spinner.
    loading_spinner.set_size_request(48, 48);
    let loading_state_container = Box::builder()
        .orientation(Vertical)
        .halign(Center)
        .valign(Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    loading_state_container.append(&loading_spinner);

    // The scanning label widget is also part of the loading state.
    let scanning_label_widget = create_scanning_label();
    loading_state_container.append(&scanning_label_widget);
    scanning_label_widget.set_visible(true);

    // --- Scanning State ---
    // Shown when a library scan is actively in progress.
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

    // Add all the different states as named children to the `artists_stack`.
    artists_stack.add_named(&loading_state_container, Some("loading_state"));
    artists_stack.add_named(&empty_state_container, Some("empty_state"));
    artists_stack.add_named(&no_results_container, Some("no_results_state"));
    artists_stack.add_named(&scanning_state_container, Some("scanning_state"));

    // The populated grid is wrapped in a vertical box.
    let artists_content_box = Box::builder().orientation(Vertical).build();
    artists_content_box.prepend(&*artist_count_label);

    // The artist_count_label is now passed from main_window/builder.rs
    artists_content_box.append(&scrolled);
    artists_stack.add_named(&artists_content_box, Some("populated_grid"));

    // Set the initial visible child of the stack to the loading state.
    artists_stack.set_visible_child_name("loading_state");
    (artists_stack, artist_grid)
}

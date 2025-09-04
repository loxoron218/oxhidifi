use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{
    Align::Start,
    Box, Button, Label, ListBox,
    Orientation::{Horizontal, Vertical},
    SelectionMode,
};
use libadwaita::{
    ViewStack,
    prelude::{BoxExt, WidgetExt},
};

use crate::ui::components::view_controls::sorting_controls::{
    handlers::connect_sort_direction_handlers, types::SortOrder, updater::update_sorting_criteria,
};

/// Creates a custom sorting control widget that integrates with the application's sorting system.
///
/// This function creates a section with:
/// 1. A label showing "Sort By"
/// 2. An icon button for toggling ascending/descending order
/// 3. A list of available sort criteria from the settings
///
/// # Arguments
///
/// * `sort_orders` - Shared reference to the current sort order preferences
/// * `sort_ascending` - Shared reference to the album sort direction
/// * `sort_ascending_artists` - Shared reference to the artist sort direction
/// * `on_sort_changed` - Callback function to refresh the UI when sorting changes
///
/// # Returns
///
/// A `gtk::Box` containing the custom sorting control widget.
pub fn create_sorting_control_row(
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    on_sort_changed: Rc<dyn Fn(bool, bool)>,
    stack: Rc<ViewStack>,
) -> Box {
    // Main Container: A vertical box for the whole sorting section.
    let main_box = Box::builder()
        .orientation(Vertical)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    // Create a horizontal box for the sorting section with label and direction button
    let sorting_box = Box::builder()
        .orientation(Horizontal)
        .margin_start(12)
        .margin_top(6)
        .margin_bottom(6)
        .spacing(12)
        .build();

    // Create a label for the sorting section
    let sorting_label = Label::builder()
        .label("Sort By")
        .halign(Start)
        .hexpand(true)
        .build();

    // Create a button group container for the sort direction button
    let sort_direction_button_box = Box::builder().orientation(Horizontal).build();
    sort_direction_button_box.add_css_class("linked");

    // Create a button for toggling sort direction
    // Set initial icon based on current sort direction of the current view
    let current_tab = stack
        .visible_child_name()
        .unwrap_or_else(|| "albums".into());
    let is_currently_albums = current_tab.as_str() == "albums";
    let initial_icon = if (is_currently_albums && sort_ascending.get())
        || (!is_currently_albums && sort_ascending_artists.get())
    {
        // For ascending order
        "view-sort-descending-symbolic"
    } else {
        // For descending order
        "view-sort-ascending-symbolic"
    };
    let sort_direction_button = Button::builder()
        .icon_name(initial_icon)
        .css_classes(["flat"])
        .tooltip_text("Toggle Sort Direction")
        .build();
    sort_direction_button_box.append(&sort_direction_button);
    sorting_box.append(&sorting_label);
    sorting_box.append(&sort_direction_button_box);
    main_box.append(&sorting_box);

    // Connect the sort direction button handlers
    connect_sort_direction_handlers(
        &sort_direction_button,
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
        on_sort_changed.clone(),
        stack.clone(),
    );

    // Create a label for the sort criteria section
    let criteria_label = Label::builder()
        .label("Drag to reorder criteria:")
        .halign(Start)
        .margin_start(12)
        .margin_top(6)
        .margin_bottom(6)
        .css_classes(vec!["dim-label"])
        .build();
    main_box.append(&criteria_label);

    // Create a ListBox for the sort criteria with drag and drop support
    let sort_listbox = ListBox::builder()
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(6)
        .build();
    sort_listbox.set_selection_mode(SelectionMode::None);
    main_box.append(&sort_listbox);

    // Create rows for each sort criterion with drag and drop support
    update_sorting_criteria(
        &sort_listbox,
        sort_orders,
        sort_ascending,
        sort_ascending_artists,
        on_sort_changed,
    );
    main_box
}

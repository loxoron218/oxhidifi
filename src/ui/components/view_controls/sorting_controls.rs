use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    str::FromStr,
    time::Duration,
};

use glib::{SendValue, Type, timeout_add_local_once};
use gtk4::{
    Align::Start,
    Box, Button, DropTarget, Label, ListBox, ListBoxRow,
    Orientation::{Horizontal, Vertical},
    gdk::{ContentProvider, DragAction},
};
use libadwaita::{
    ActionRow,
    prelude::{
        BoxExt, ButtonExt, Cast, ListBoxRowExt, ObjectExt, ObjectType, PreferencesRowExt, WidgetExt,
    },
};

use crate::ui::components::{
    config::{Settings, load_settings, save_settings},
    sorting::sorting_types::{SortOrder, sort_order_label},
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
    // Set initial icon based on current sort direction
    let initial_icon = if sort_ascending.get() {
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

    // Clone references for the callbacks
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let on_sort_changed_clone = on_sort_changed.clone();
    let sort_direction_button_clone = sort_direction_button.clone();

    // Connect the sort direction button to update sort direction
    sort_direction_button.connect_clicked(move |_| {
        // Toggle the sort direction
        let is_ascending = !sort_ascending_clone.get();

        // Update the shared state
        sort_ascending_clone.set(is_ascending);
        sort_ascending_artists_clone.set(is_ascending);

        // Update the button icon
        let icon_name = if is_ascending {
            // For ascending order
            "view-sort-descending-symbolic"
        } else {
            // For descending order
            "view-sort-ascending-symbolic"
        };
        sort_direction_button_clone.set_icon_name(icon_name);

        // Save to settings
        let mut settings = load_settings();
        settings.sort_ascending_albums = is_ascending;
        settings.sort_ascending_artists = is_ascending;
        let _ = save_settings(&settings);

        // Trigger UI refresh
        on_sort_changed_clone(
            sort_ascending_clone.get(),
            sort_ascending_artists_clone.get(),
        );
    });

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
    sort_listbox.set_selection_mode(gtk4::SelectionMode::None);
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

/// Updates the sorting criteria in the popover
///
/// This function creates the sort criterion buttons with drag and drop support
/// and adds them to the main box.
///
/// # Arguments
///
/// * `sort_listbox` - The ListBox to add the sort criterion rows to
/// * `sort_orders` - Shared reference to the current sort order preferences
/// * `sort_ascending` - Shared reference to the album sort direction
/// * `sort_ascending_artists` - Shared reference to the artist sort direction
/// * `on_sort_changed` - Callback function to refresh the UI when sorting changes
pub fn update_sorting_criteria(
    sort_listbox: &ListBox,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    on_sort_changed: Rc<dyn Fn(bool, bool)>,
) {
    // Clear existing rows
    while let Some(child) = sort_listbox.first_child() {
        sort_listbox.remove(&child);
    }

    // Clone references for the callbacks
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let on_sort_changed_clone = on_sort_changed.clone();

    // Create rows for each sort criterion with drag and drop support
    let current_orders = sort_orders.borrow();
    for (index, order) in current_orders.iter().enumerate() {
        // Create an ActionRow for each sort criterion
        let numbered_label = format!("{}. {}", index + 1, sort_order_label(order));
        let row = ActionRow::builder().title(numbered_label).build();
        row.set_widget_name(&sort_order_label(order));
        let list_row = ListBoxRow::new();
        list_row.set_child(Some(&row));

        // DragSource setup
        let drag_source = gtk4::DragSource::new();
        drag_source.set_actions(DragAction::MOVE);
        drag_source.connect_prepare({
            let order_clone = *order;
            move |_, _, _| {
                let order_str = sort_order_label(&order_clone).to_string();
                let value = SendValue::from(&order_str);
                Some(ContentProvider::for_value(&value))
            }
        });
        list_row.add_controller(drag_source);

        // DropTarget setup
        let drop_target = DropTarget::new(Type::STRING, DragAction::MOVE);
        drop_target.connect_drop({
            let list_row_weak = list_row.downgrade();
            let sort_orders_rc = sort_orders.clone();
            let refresh_library_ui_cb = on_sort_changed_clone.clone();
            let sort_ascending_clone_drop = sort_ascending_clone.clone();
            let sort_ascending_artists_clone_drop = sort_ascending_artists_clone.clone();
            let sort_listbox_clone = sort_listbox.clone();
            let sort_orders_clone_drop = sort_orders.clone();
            let sort_ascending_clone_drop2 = sort_ascending.clone();
            let sort_ascending_artists_clone_drop2 = sort_ascending_artists.clone();
            let on_sort_changed_clone_drop = on_sort_changed.clone();
            move |_target_row, value, _x, _y| {
                if let Ok(dragged_order_str) = value.get::<String>() {
                    if let Some(target_row) = list_row_weak.upgrade() {
                        if let Some(listbox) = target_row
                            .parent()
                            .and_then(|p| p.downcast::<ListBox>().ok())
                        {
                            // Find the source and target indices for reordering
                            let mut count = 0;
                            let mut current_child = listbox.first_child();
                            while let Some(child) = current_child {
                                count += 1;
                                current_child = child.next_sibling();
                            }
                            let mut children_widgets: Vec<gtk4::Widget> = Vec::with_capacity(count);
                            let mut current_child = listbox.first_child();
                            while let Some(child) = current_child {
                                children_widgets.push(child.clone());
                                current_child = child.next_sibling();
                            }
                            let mut source_idx = None;
                            let mut target_idx = None;
                            for (idx, child_widget) in children_widgets.iter().enumerate() {
                                if child_widget.as_ptr()
                                    == target_row.clone().upcast_ref::<gtk4::Widget>().as_ptr()
                                {
                                    target_idx = Some(idx);
                                }
                                if let Some(list_row_ref) =
                                    child_widget.downcast_ref::<ListBoxRow>()
                                {
                                    if let Some(action_row_ref) = list_row_ref
                                        .child()
                                        .and_then(|c| c.downcast::<ActionRow>().ok())
                                    {
                                        let order_str_val = action_row_ref.widget_name();
                                        if !order_str_val.is_empty() {
                                            if order_str_val == dragged_order_str {
                                                source_idx = Some(idx);
                                            }
                                        }
                                    }
                                }
                            }
                            if let (Some(from), Some(to)) = (source_idx, target_idx) {
                                if from != to {
                                    let row_to_move = &children_widgets[from];
                                    listbox.remove(row_to_move);

                                    // Re-insert at the new position
                                    // The `insert` method handles the out-of-bounds case by appending,
                                    listbox.insert(row_to_move, to as i32);

                                    // Update sort_orders, persist, and refresh UI after reorder
                                    let mut new_orders = Vec::with_capacity(children_widgets.len());
                                    let mut current_child_after_reorder = listbox.first_child();
                                    while let Some(child_after_reorder) =
                                        current_child_after_reorder
                                    {
                                        if let Some(list_row_ref) =
                                            child_after_reorder.downcast_ref::<ListBoxRow>()
                                        {
                                            if let Some(action_row_ref) = list_row_ref
                                                .child()
                                                .and_then(|c| c.downcast::<ActionRow>().ok())
                                            {
                                                let order_str_val = action_row_ref.widget_name();
                                                if !order_str_val.is_empty() {
                                                    if let Ok(order) =
                                                        SortOrder::from_str(&order_str_val)
                                                    {
                                                        new_orders.push(order);
                                                    }
                                                }
                                            }
                                        }
                                        current_child_after_reorder =
                                            child_after_reorder.next_sibling();
                                    }
                                    if !new_orders.is_empty() {
                                        *sort_orders_rc.borrow_mut() = new_orders.clone();
                                        let prev = load_settings();
                                        let _ = save_settings(&Settings {
                                            sort_orders: new_orders,
                                            sort_ascending_albums: prev.sort_ascending_albums,
                                            sort_ascending_artists: prev.sort_ascending_artists,
                                            completed_albums: prev.completed_albums,
                                            show_dr_badges: prev.show_dr_badges,
                                            use_original_year: prev.use_original_year,
                                            view_mode: prev.view_mode.clone(),
                                        });

                                        // Trigger UI refresh
                                        (refresh_library_ui_cb)(
                                            sort_ascending_clone_drop.get(),
                                            sort_ascending_artists_clone_drop.get(),
                                        );

                                        // Schedule UI update to avoid RefCell borrowing issues
                                        let sort_listbox_clone2 = sort_listbox_clone.clone();
                                        let sort_orders_clone_drop2 =
                                            sort_orders_clone_drop.clone();
                                        let sort_ascending_clone_drop3 =
                                            sort_ascending_clone_drop2.clone();
                                        let sort_ascending_artists_clone_drop3 =
                                            sort_ascending_artists_clone_drop2.clone();
                                        let on_sort_changed_clone_drop2 =
                                            on_sort_changed_clone_drop.clone();
                                        timeout_add_local_once(
                                            Duration::from_millis(1),
                                            move || {
                                                update_popover_content(
                                                    &sort_listbox_clone2,
                                                    sort_orders_clone_drop2,
                                                    sort_ascending_clone_drop3,
                                                    sort_ascending_artists_clone_drop3,
                                                    on_sort_changed_clone_drop2,
                                                );
                                            },
                                        );
                                    } else {
                                        // Update numbering in ActionRow titles
                                        update_sorting_row_numbers(&listbox);
                                    }
                                }
                            }
                        }
                    }
                }
                true
            }
        });
        list_row.add_controller(drop_target);
        sort_listbox.append(&list_row);
    }
}

/// Updates the popover content with the current sort orders
///
/// This function clears the existing sort criterion buttons and recreates them
/// with the updated sort orders.
///
/// # Arguments
///
/// * `sort_listbox` - The ListBox containing the sort criterion rows
/// * `sort_orders` - Shared reference to the current sort order preferences
/// * `sort_ascending` - Shared reference to the album sort direction
/// * `sort_ascending_artists` - Shared reference to the artist sort direction
/// * `on_sort_changed` - Callback function to refresh the UI when sorting changes
pub fn update_popover_content(
    sort_listbox: &ListBox,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    on_sort_changed: Rc<dyn Fn(bool, bool)>,
) {
    // Update the sort criterion rows
    update_sorting_criteria(
        sort_listbox,
        sort_orders,
        sort_ascending,
        sort_ascending_artists,
        on_sort_changed,
    );
}

/// Updates the numbering displayed in the titles of `ActionRow` widgets within a `ListBox`.
///
/// This function iterates through each `ListBoxRow` in the provided `ListBox`, extracts
/// the original sort order string from its data, and then updates the `ActionRow`'s title
/// to include a sequential number (e.g., "1. Artist", "2. Year"). This is useful for
/// providing visual feedback on the order of items in a sort preference list.
///
/// # Arguments
///
/// * `listbox` - The `ListBox` containing the `ActionRow` widgets to be renumbered.
pub fn update_sorting_row_numbers(listbox: &ListBox) {
    let mut child = listbox.first_child();
    let mut idx = 1;
    while let Some(row) = child {
        if let Some(list_row) = row.downcast_ref::<ListBoxRow>() {
            if let Some(action_row) = list_row
                .child()
                .and_then(|c| c.downcast::<ActionRow>().ok())
            {
                let order_str = action_row.widget_name();
                if !order_str.is_empty() {
                    let label = format!("{}. {}", idx, order_str);
                    action_row.set_title(&label);
                }
            }
        }
        child = row.next_sibling();
        idx += 1;
    }
}

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{ListBox, ListBoxRow};
use libadwaita::{
    ActionRow,
    prelude::{Cast, ListBoxRowExt, PreferencesRowExt, WidgetExt},
};

use crate::ui::components::view_controls::sorting_controls::{
    drag_drop::setup_drag_drop_for_row,
    types::{SortOrder, sort_order_label},
};

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
        row.set_widget_name(sort_order_label(order));
        let list_row = ListBoxRow::new();
        list_row.set_child(Some(&row));

        // Set up drag and drop for the row
        setup_drag_drop_for_row(
            &list_row,
            *order,
            sort_listbox,
            sort_orders.clone(),
            sort_ascending_clone.clone(),
            sort_ascending_artists_clone.clone(),
            on_sort_changed_clone.clone(),
        );
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
        if let Some(list_row) = row.downcast_ref::<ListBoxRow>()
            && let Some(action_row) = list_row
                .child()
                .and_then(|c| c.downcast::<ActionRow>().ok())
        {
            let order_str = action_row.widget_name();
            if !order_str.is_empty() {
                let label = format!("{}. {}", idx, order_str);
                action_row.set_title(&label);
            }
        }
        child = row.next_sibling();
        idx += 1;
    }
}

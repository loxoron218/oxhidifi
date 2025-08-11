use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    str::FromStr,
};

use glib::{Type, Value, source::idle_add_local_once};
use gtk4::{DragSource, DropTarget, ListBox, ListBoxRow, Widget};
use libadwaita::{
    ActionRow,
    gdk::{ContentProvider, DragAction},
    prelude::{Cast, ListBoxRowExt, ObjectExt, ObjectType, PreferencesRowExt, WidgetExt},
};

use crate::ui::components::config::{Settings, load_settings, save_settings};

use super::{
    sorting_types::{SortOrder, sort_order_label},
    sorting_ui_utils::get_sort_icon_name,
};

/// Helper: create a `ListBoxRow` with DnD enabled
///
/// This function creates a `ListBoxRow` for a given `SortOrder`, setting up
/// drag-and-drop functionality to allow reordering of sort preferences.
/// It also handles the logic for updating the `sort_orders` in settings
/// and refreshing the UI after a successful drop.
///
/// # Arguments
///
/// * `order` - The `SortOrder` variant this row represents.
/// * `sort_orders` - An `Rc<RefCell<Vec<SortOrder>>>` holding the current sort order preferences.
/// * `refresh_library_ui` - A callback `Rc<dyn Fn(bool, bool)>` to refresh the main library UI.
/// * `sort_ascending` - An `Rc<Cell<bool>>` indicating the sort direction for albums.
/// * `sort_ascending_artists` - An `Rc<Cell<bool>>` indicating the sort direction for artists.
///
/// # Returns
///
/// A `ListBoxRow` configured for drag-and-drop.
#[allow(clippy::too_many_arguments)]
pub fn make_sort_row(
    order: &SortOrder,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) -> ListBoxRow {
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let row = ActionRow::builder().title(sort_order_label(order)).build();
    unsafe {
        row.set_data("sort-order", sort_order_label(order).to_string());
    }
    let list_row = ListBoxRow::new();
    list_row.set_child(Some(&row));

    // DragSource setup
    let drag_source = DragSource::new();
    drag_source.set_actions(DragAction::MOVE);
    drag_source.connect_prepare({
        let order_clone = order.clone();
        move |_, _, _| {
            let order_str = sort_order_label(&order_clone).to_string();
            let value = Value::from(order_str);
            Some(ContentProvider::for_value(&value))
        }
    });
    list_row.add_controller(drag_source);

    // DropTarget setup
    let drop_target = DropTarget::new(Type::STRING, DragAction::MOVE);
    drop_target.connect_drop({
        let list_row_weak = list_row.downgrade();
        move |_target_row, value, _x, _y| {
            let sort_orders_rc = sort_orders.clone();
            let refresh_library_ui_cb = refresh_library_ui.clone();
            if let Ok(dragged_order_str) = value.get::<String>() {
                if let Some(target_row) = list_row_weak.upgrade() {
                    if let Some(listbox) = target_row
                        .parent()
                        .and_then(|p| p.downcast::<ListBox>().ok())
                    {
                        // Find the source and target indices for reordering
                        let mut children_widgets: Vec<Widget> = Vec::new();
                        let mut current_child = listbox.first_child();
                        while let Some(child) = current_child {
                            children_widgets.push(child.clone());
                            current_child = child.next_sibling();
                        }

                        let mut source_idx = None;
                        let mut target_idx = None;

                        for (idx, child_widget) in children_widgets.iter().enumerate() {
                            if child_widget.as_ptr()
                                == target_row.clone().upcast::<Widget>().as_ptr()
                            {
                                target_idx = Some(idx);
                            }
                            if let Some(list_row_ref) = child_widget.downcast_ref::<ListBoxRow>() {
                                if let Some(action_row_ref) = list_row_ref
                                    .child()
                                    .and_then(|c| c.downcast::<ActionRow>().ok())
                                {
                                    if let Some(order_str_data) =
                                        unsafe { action_row_ref.data::<String>("sort-order") }
                                    {
                                        let order_str_val = unsafe { order_str_data.as_ref() };
                                        if *order_str_val == dragged_order_str {
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
                                if to >= children_widgets.len() {
                                    // Use children_widgets.len() for current count
                                    listbox.append(row_to_move);
                                } else {
                                    listbox.insert(row_to_move, to as i32);
                                }

                                // Update sort_orders, persist, and refresh UI after reorder
                                let mut new_orders = Vec::new();
                                let mut current_child_after_reorder = listbox.first_child();
                                while let Some(child_after_reorder) = current_child_after_reorder {
                                    if let Some(list_row_ref) =
                                        child_after_reorder.downcast_ref::<ListBoxRow>()
                                    {
                                        if let Some(action_row_ref) = list_row_ref
                                            .child()
                                            .and_then(|c| c.downcast::<ActionRow>().ok())
                                        {
                                            if let Some(order_str_data) = unsafe {
                                                action_row_ref.data::<String>("sort-order")
                                            } {
                                                let order_str_val =
                                                    unsafe { order_str_data.as_ref() };
                                                if let Ok(order) =
                                                    SortOrder::from_str(order_str_val)
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
                                    });

                                    // Update numbering in ActionRow titles
                                    update_sorting_row_numbers(&listbox);
                                    (refresh_library_ui_cb)(
                                        get_sort_icon_name(
                                            "albums",
                                            &sort_ascending_clone,
                                            &sort_ascending_artists_clone,
                                        ) == "view-sort-descending-symbolic",
                                        get_sort_icon_name(
                                            "artists",
                                            &sort_ascending_clone,
                                            &sort_ascending_artists_clone,
                                        ) == "view-sort-descending-symbolic",
                                    );
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
    list_row
}

/// Connects a reorder handler to the given ListBox for SortOrder persistence and UI refresh.
///
/// This function listens for the `map` signal on the `sort_listbox`. When the ListBox
/// is mapped (e.g., when it becomes visible), it collects the current order of the
/// `SortOrder` rows, updates the shared `sort_orders` `RefCell`, and persists these
/// changes to the application settings.
///
/// This ensures that changes made via drag-and-drop are saved and reflected in the UI.
///
/// # Arguments
///
/// * `sort_listbox` - The `ListBox` containing the `SortOrder` rows.
/// * `sort_orders` - An `Rc<RefCell<Vec<SortOrder>>>` holding the current sort order preferences.
pub fn connect_sort_reorder_handler(
    sort_listbox: &ListBox,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
) {
    let sort_orders_rc = sort_orders.clone();
    let sort_listbox_weak = sort_listbox.downgrade();
    sort_listbox.connect_map(move |_listbox| {
        let sort_orders_rc_clone = sort_orders_rc.clone();
        let sort_listbox_weak_clone = sort_listbox_weak.clone();
        idle_add_local_once(move || {
            if let Some(listbox) = sort_listbox_weak_clone.upgrade() {
                let mut new_orders = Vec::new();
                let mut current_child = listbox.first_child();
                while let Some(row) = current_child {
                    if let Some(list_row) = row.downcast_ref::<ListBoxRow>() {
                        if let Some(action_row) = list_row
                            .child()
                            .and_then(|c| c.downcast::<ActionRow>().ok())
                        {
                            if let Some(order_str_nn) =
                                unsafe { action_row.data::<String>("sort-order") }
                            {
                                let order_str = unsafe { order_str_nn.as_ref() };
                                if let Ok(order) = SortOrder::from_str(order_str) {
                                    new_orders.push(order);
                                }
                            }
                        }
                    }
                    current_child = row.next_sibling();
                }
                if !new_orders.is_empty() {
                    *sort_orders_rc_clone.borrow_mut() = new_orders.clone();
                    let prev = load_settings();
                    let _ = save_settings(&Settings {
                        sort_orders: new_orders,
                        sort_ascending_albums: prev.sort_ascending_albums,
                        sort_ascending_artists: prev.sort_ascending_artists,
                        completed_albums: prev.completed_albums,
                        show_dr_badges: prev.show_dr_badges,
                    });
                }
            }
        });
    });
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
                if let Some(order_str_nn) = unsafe { action_row.data::<String>("sort-order") } {
                    let order_str = unsafe { order_str_nn.as_ref() };
                    let label = format!("{}. {}", idx, order_str);
                    action_row.set_title(&label);
                }
            }
        }
        child = row.next_sibling();
        idx += 1;
    }
}

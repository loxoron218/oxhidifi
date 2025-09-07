use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    str::FromStr,
    time::Duration,
};

use glib::{SendValue, Type, clone::Downgrade, timeout_add_local_once};
use gtk4::{
    DragSource, DropTarget, ListBox, ListBoxRow, Widget,
    gdk::{ContentProvider, DragAction},
};
use libadwaita::{
    ActionRow,
    prelude::{Cast, ListBoxRowExt, ObjectType, WidgetExt},
};

use crate::ui::components::{
    config::{load_settings, save_settings},
    view_controls::sorting_controls::{
        types::{SortOrder, sort_order_label},
        updater::{update_popover_content, update_sorting_row_numbers},
    },
};

/// Sets up drag and drop functionality for a sort criterion row
///
/// This function configures the drag source and drop target for a ListBoxRow
/// representing a sort criterion, enabling reordering through drag and drop.
///
/// # Arguments
///
/// * `list_row` - The ListBoxRow to set up drag and drop for
/// * `order` - The SortOrder represented by this row
/// * `sort_listbox` - The parent ListBox containing all sort criterion rows
/// * `sort_orders` - Shared reference to the current sort order preferences
/// * `sort_ascending` - Shared reference to the album sort direction
/// * `sort_ascending_artists` - Shared reference to the artist sort direction
/// * `on_sort_changed` - Callback function to refresh the UI when sorting changes
pub fn setup_drag_drop_for_row(
    list_row: &ListBoxRow,
    order: SortOrder,
    sort_listbox: &ListBox,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    on_sort_changed: Rc<dyn Fn(bool, bool)>,
) {
    // DragSource setup
    let drag_source = DragSource::new();
    drag_source.set_actions(DragAction::MOVE);
    drag_source.connect_prepare({
        let order_clone = order;
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
        let list_row_weak = Downgrade::downgrade(list_row);
        let sort_orders_rc = sort_orders.clone();
        let refresh_library_ui_cb = on_sort_changed.clone();
        let sort_ascending_clone_drop = sort_ascending.clone();
        let sort_ascending_artists_clone_drop = sort_ascending_artists.clone();
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
                        let mut children_widgets: Vec<Widget> = Vec::with_capacity(count);
                        let mut current_child = listbox.first_child();
                        while let Some(child) = current_child {
                            children_widgets.push(child.clone());
                            current_child = child.next_sibling();
                        }
                        let mut source_idx = None;
                        let mut target_idx = None;
                        for (idx, child_widget) in children_widgets.iter().enumerate() {
                            if child_widget.as_ptr()
                                == target_row.clone().upcast_ref::<Widget>().as_ptr()
                            {
                                target_idx = Some(idx);
                            }
                            if let Some(list_row_ref) = child_widget.downcast_ref::<ListBoxRow>() {
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
                                while let Some(child_after_reorder) = current_child_after_reorder {
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
                                    let mut settings = load_settings();
                                    settings.sort_orders = new_orders;
                                    settings.sort_ascending_albums = prev.sort_ascending_albums;
                                    settings.sort_ascending_artists = prev.sort_ascending_artists;
                                    settings.completed_albums = prev.completed_albums;
                                    settings.show_dr_badges = prev.show_dr_badges;
                                    settings.use_original_year = prev.use_original_year;
                                    settings.view_mode = prev.view_mode;
                                    let _ = save_settings(&settings);

                                    // Trigger UI refresh
                                    (refresh_library_ui_cb)(
                                        sort_ascending_clone_drop.get(),
                                        sort_ascending_artists_clone_drop.get(),
                                    );

                                    // Schedule UI update to avoid RefCell borrowing issues
                                    let sort_listbox_clone2 = sort_listbox_clone.clone();
                                    let sort_orders_clone_drop2 = sort_orders_clone_drop.clone();
                                    let sort_ascending_clone_drop3 =
                                        sort_ascending_clone_drop2.clone();
                                    let sort_ascending_artists_clone_drop3 =
                                        sort_ascending_artists_clone_drop2.clone();
                                    let on_sort_changed_clone_drop2 =
                                        on_sort_changed_clone_drop.clone();
                                    timeout_add_local_once(Duration::from_millis(1), move || {
                                        update_popover_content(
                                            &sort_listbox_clone2,
                                            sort_orders_clone_drop2,
                                            sort_ascending_clone_drop3,
                                            sort_ascending_artists_clone_drop3,
                                            on_sort_changed_clone_drop2,
                                        );
                                    });
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
}

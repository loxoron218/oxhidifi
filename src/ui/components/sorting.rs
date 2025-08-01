use std::cell::{Cell, RefCell};
use std::{rc::Rc, str::FromStr};

use glib::source::idle_add_local_once;
use glib::{Type, Value};
use gtk4::gdk::{ContentProvider, DragAction};
use gtk4::prelude::{ButtonExt, ToggleButtonExt};
use gtk4::{Button, DragSource, DropTarget, ListBox, ListBoxRow, ToggleButton, Widget};
use libadwaita::prelude::{
    Cast, ListBoxRowExt, ObjectExt, ObjectType, PreferencesRowExt, WidgetExt,
};
use libadwaita::{ActionRow, ViewStack};
use serde::{Deserialize, Serialize};

use crate::ui::components::config::{Settings, load_settings, save_settings};

/// Connect toggled handlers for albums and artists tab buttons to refresh sorting.
pub fn connect_tab_sort_refresh(
    albums_btn: &ToggleButton,
    artists_btn: &ToggleButton,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    let refresh_library_ui_albums = refresh_library_ui.clone();
    let sort_ascending_albums = sort_ascending.clone();
    let sort_ascending_artists_albums = sort_ascending_artists.clone();
    albums_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            refresh_library_ui_albums(
                sort_ascending_albums.get(),
                sort_ascending_artists_albums.get(),
            );
        }
    });
    let refresh_library_ui_artists = refresh_library_ui.clone();
    let sort_ascending_artists_btn = sort_ascending.clone();
    let sort_ascending_artists_val = sort_ascending_artists.clone();
    artists_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            refresh_library_ui_artists(
                sort_ascending_artists_btn.get(),
                sort_ascending_artists_val.get(),
            );
        }
    });
}

/// Connects a handler to update the sort icon on tab switch.
pub fn connect_sort_icon_update_on_tab_switch(
    sort_button: &Button,
    stack: &ViewStack,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    let sort_button = sort_button.clone();
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
        let page = stack.visible_child_name().unwrap_or_default();
        if page == "artists" {
            sort_button.set_icon_name(if sort_ascending_artists.get() {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
        } else {
            sort_button.set_icon_name(if sort_ascending.get() {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
        }
    });
}

/// Represents the sorting order for library views.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum SortOrder {
    Artist,
    Album,
    Year,
    Format,
}

/// Allows parsing a SortOrder from a string ("Artist", "Year", etc). Useful for persistence and drag-and-drop.
impl FromStr for SortOrder {
    type Err = ();

    // Allows conversion from string to SortOrder for persistence, drag-and-drop, and UI.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Artist" => Ok(SortOrder::Artist),
            "Year" => Ok(SortOrder::Year),
            "Album" => Ok(SortOrder::Album),
            "Format" => Ok(SortOrder::Format),
            _ => Err(()),
        }
    }
}

/// Set the initial sort icon state for the sort button based on the current page and sort order.
pub fn set_initial_sort_icon_state(
    sort_button: &impl ButtonExt,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
    initial_page: &str,
) {
    let icon_name = if initial_page == "artists" {
        if sort_ascending_artists.get() {
            "view-sort-descending-symbolic"
        } else {
            "view-sort-ascending-symbolic"
        }
    } else {
        if sort_ascending.get() {
            "view-sort-descending-symbolic"
        } else {
            "view-sort-ascending-symbolic"
        }
    };
    sort_button.set_icon_name(icon_name);
}

/// Returns the display label for a given SortOrder variant (for UI).
fn sort_order_label(order: &SortOrder) -> &'static str {
    match order {
        SortOrder::Artist => "Artist",
        SortOrder::Year => "Year",
        SortOrder::Album => "Album",
        SortOrder::Format => "Format",
    }
}

/// Helper: create a ListBoxRow with DnD enabled
pub fn make_sort_row(
    order: &SortOrder,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) -> ListBoxRow {
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    let row = ActionRow::builder().title(sort_order_label(order)).build();
    unsafe {
        row.set_data("sort-order", sort_order_label(order).to_string());
    }
    let list_row = ListBoxRow::new();
    list_row.set_child(Some(&row));

    // DragSource
    let drag_source = DragSource::new();
    drag_source.set_actions(DragAction::MOVE);
    drag_source.connect_prepare({
        let order = order.clone();
        move |_, _, _| {
            let order_str = format!("{:?}", order);
            let value = Value::from(order_str);
            Some(ContentProvider::for_value(&value))
        }
    });
    list_row.add_controller(drag_source);

    // DropTarget
    let drop_target = DropTarget::new(Type::STRING, DragAction::MOVE);
    drop_target.connect_drop({
        let list_row_weak = list_row.downgrade();
        move |_target_row, value, _x, _y| {
            let sort_orders_rc = sort_orders.clone();
            let refresh_library_ui_cb = refresh_library_ui.clone();
            if let Ok(_order_str) = value.get::<String>() {
                if let Some(target_row) = list_row_weak.upgrade() {
                    let listbox_weak = target_row
                        .parent()
                        .and_then(|p| p.downcast::<ListBox>().ok())
                        .map(|lb| lb.downgrade());
                    if let Some(listbox_weak) = listbox_weak {
                        if let Some(listbox) = listbox_weak.upgrade() {
                            // Collect all children
                            let mut rows: Vec<Widget> = Vec::new();
                            let mut child = listbox.first_child();
                            while let Some(row) = child {
                                rows.push(row.clone());
                                child = row.next_sibling();
                            }
                            let mut source_idx = None;
                            let mut target_idx = None;
                            for (idx, row) in rows.iter().enumerate() {
                                if row.as_ptr() == target_row.clone().upcast::<Widget>().as_ptr() {
                                    target_idx = Some(idx);
                                }
                                if let Some(list_row) = row.downcast_ref::<ListBoxRow>() {
                                    if let Some(action_row) = list_row
                                        .child()
                                        .and_then(|c| c.downcast::<ActionRow>().ok())
                                    {
                                        if let Some(order_str_nn) =
                                            unsafe { action_row.data::<String>("sort-order") }
                                        {
                                            let order_str = unsafe { order_str_nn.as_ref() };
                                            if *order_str == _order_str {
                                                source_idx = Some(idx);
                                            }
                                        }
                                    }
                                }
                            }
                            if let (Some(from), Some(to)) = (source_idx, target_idx) {
                                if from != to {
                                    let row_to_move = &rows[from];
                                    listbox.remove(row_to_move);

                                    // Re-collect after removal
                                    let mut rows: Vec<Widget> = Vec::new();
                                    let mut child = listbox.first_child();
                                    while let Some(row) = child {
                                        rows.push(row.clone());
                                        child = row.next_sibling();
                                    }
                                    if to >= rows.len() {
                                        listbox.append(row_to_move);
                                    } else {
                                        listbox.insert(row_to_move, to as i32);
                                    }

                                    // After reorder, update sort_orders, persist, and refresh
                                    let mut new_orders = Vec::new();
                                    let mut child = listbox.first_child();
                                    while let Some(row) = child {
                                        if let Some(list_row) = row.downcast_ref::<ListBoxRow>() {
                                            if let Some(action_row) = list_row
                                                .child()
                                                .and_then(|c| c.downcast::<ActionRow>().ok())
                                            {
                                                if let Some(order_str_nn) = unsafe {
                                                    action_row.data::<String>("sort-order")
                                                } {
                                                    let order_str =
                                                        unsafe { order_str_nn.as_ref() };
                                                    if let Ok(order) =
                                                        SortOrder::from_str(order_str)
                                                    {
                                                        new_orders.push(order);
                                                    }
                                                }
                                            }
                                        }
                                        child = row.next_sibling();
                                    }
                                    if !new_orders.is_empty() {
                                        *sort_orders_rc.borrow_mut() = new_orders.clone();
                                        let prev = load_settings();
                                        let _ = save_settings(&Settings {
                                            sort_orders: new_orders,
                                            sort_ascending_albums: prev.sort_ascending_albums,
                                            sort_ascending_artists: prev.sort_ascending_artists,
                                            completed_albums: prev.completed_albums,
                                        });

                                        // Update numbering in ActionRow titles
                                        update_sorting_row_numbers(&listbox);
                                        (refresh_library_ui_cb)(
                                            sort_ascending.get(),
                                            sort_ascending_artists.get(),
                                        );
                                    }
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
/// This wires up the drag-and-drop reorder logic, updates the shared sort_orders, persists, and refreshes the UI.
pub fn connect_sort_reorder_handler(
    sort_listbox: &ListBox,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
) {
    let sort_orders_rc = sort_orders.clone();
    let sort_listbox_weak = sort_listbox.downgrade();
    sort_listbox.connect_map(move |_listbox| {
        let sort_orders_rc = sort_orders_rc.clone();
        let sort_listbox_weak = sort_listbox_weak.clone();
        idle_add_local_once(move || {
            if let Some(listbox) = sort_listbox_weak.upgrade() {
                let mut new_orders = Vec::new();
                let mut rows: Vec<Widget> = Vec::new();
                let mut child = listbox.first_child();
                while let Some(row) = child {
                    rows.push(row.clone());
                    child = row.next_sibling();
                }
                for row in rows {
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
                }
                if !new_orders.is_empty() {
                    *sort_orders_rc.borrow_mut() = new_orders.clone();
                    let prev = load_settings();
                    let _ = save_settings(&Settings {
                        sort_orders: new_orders,
                        sort_ascending_albums: prev.sort_ascending_albums,
                        sort_ascending_artists: prev.sort_ascending_artists,
                        completed_albums: prev.completed_albums,
                    });
                }
            }
        });
    });
}

/// Helper: update numbering in ActionRow titles
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

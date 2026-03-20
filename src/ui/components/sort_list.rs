//! Sort configuration list component.
//!
//! Provides a drag-and-drop sortable list for configuring the grid view
//! sorting criteria.

use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc, sync::Arc};

use {
    libadwaita::gtk::{
        Align::{Center, Start},
        Box, Button, DragSource, DropTarget, Image, Label, ListBox, ListBoxRow,
        Orientation::Horizontal,
        SelectionMode::None,
        gdk::{ContentProvider, DragAction},
        prelude::*,
    },
    tracing::error,
};

use crate::{
    config::settings::{
        AlbumGridSortCriteria::{self, Artist, BitDepth, DRValue, Format, SampleRate, Title, Year},
        AlbumGridSortItem,
        ArtistGridSortCriteria::{self, AlbumCount, Name},
        ArtistGridSortItem,
        SortOrder::{self, Ascending, Descending},
    },
    state::app_state::AppState,
};

/// Implements `SortItem` trait for a sort item type.
macro_rules! impl_sort_item {
    ($ty:ty, $criteria:ty) => {
        impl SortItem for $ty {
            type Criteria = $criteria;

            fn criteria(&self) -> &Self::Criteria {
                &self.criteria
            }

            fn order(&self) -> &SortOrder {
                &self.order
            }
        }
    };
}

/// Trait for sort item types with criteria and order.
trait SortItem {
    /// The criteria type for this sort item.
    type Criteria: Clone + Hash + Eq + ToString;
    /// Returns the criteria.
    fn criteria(&self) -> &Self::Criteria;
    /// Returns the order.
    fn order(&self) -> &SortOrder;
}

impl_sort_item!(AlbumGridSortItem, AlbumGridSortCriteria);
impl_sort_item!(ArtistGridSortItem, ArtistGridSortCriteria);

/// Generic sort list builder.
///
/// # Arguments
///
/// * `sort_items` - The initial sort items to display
/// * `reconstruct` - Function to reconstruct sort items from the `ListBox` order
/// * `update` - Callback to update the settings with new sort order
fn build_sort_list<T, F, U>(sort_items: Vec<T>, reconstruct: F, update: U) -> ListBox
where
    T: Clone + SortItem,
    T::Criteria: Clone + Hash + Eq + ToString + 'static,
    F: Fn(&ListBox, &HashMap<T::Criteria, SortOrder>) -> Vec<T> + Clone + 'static,
    U: Fn(Vec<T>) + Clone + 'static,
{
    let list_box = ListBox::builder()
        .selection_mode(None)
        .css_classes(["boxed-list", "sort-list"])
        .build();

    let order_map: Rc<RefCell<HashMap<T::Criteria, SortOrder>>> =
        Rc::new(RefCell::new(HashMap::with_capacity(sort_items.len())));
    for item in &sort_items {
        order_map
            .borrow_mut()
            .insert(item.criteria().clone(), *item.order());
    }

    for item in sort_items {
        let row = ListBoxRow::builder().css_classes(["sort-row"]).build();
        row.set_widget_name(&item.criteria().to_string());

        let row_box = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();

        // Drag handle
        let drag_handle = Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["dim-label"])
            .build();
        row_box.append(&drag_handle);

        // Label
        let label = Label::builder()
            .label(item.criteria().to_string())
            .halign(Start)
            .hexpand(true)
            .build();
        row_box.append(&label);

        // Toggle order button
        let toggle = Button::builder()
            .icon_name(if item.order() == &Ascending {
                "pan-up-symbolic"
            } else {
                "pan-down-symbolic"
            })
            .css_classes(["flat", "circular"])
            .valign(Center)
            .build();

        let criteria_clone = item.criteria().clone();
        let toggle_clone = toggle.clone();
        let lb_clone = list_box.clone();
        let om_clone = Rc::clone(&order_map);
        let reconstruct_clone = reconstruct.clone();
        let update_clone = update.clone();

        toggle.connect_clicked(move |_| {
            let mut om = om_clone.borrow_mut();
            let new_order = if *om.get(&criteria_clone).unwrap_or(&Ascending) == Ascending {
                Descending
            } else {
                Ascending
            };
            om.insert(criteria_clone.clone(), new_order);
            toggle_clone.set_icon_name(if new_order == Ascending {
                "pan-up-symbolic"
            } else {
                "pan-down-symbolic"
            });

            // Reconstruct and save
            let new_sort = reconstruct_clone(&lb_clone, &om);
            update_clone(new_sort);
        });

        row_box.append(&toggle);
        row.set_child(Some(&row_box));

        // Drag source
        let drag_source = DragSource::new();
        drag_source.set_actions(DragAction::MOVE);
        let row_clone = row.clone();
        drag_source.connect_prepare(move |_, _, _| {
            Some(ContentProvider::for_value(&row_clone.to_value()))
        });
        row.add_controller(drag_source);

        // Drop target
        let drop_target = DropTarget::new(ListBoxRow::static_type(), DragAction::MOVE);
        drop_target.set_preload(true);
        let lb_clone2 = list_box.clone();
        let target_row_clone = row.clone();
        let om_clone2 = Rc::clone(&order_map);
        let reconstruct_clone2 = reconstruct.clone();
        let update_clone2 = update.clone();

        drop_target.connect_drop(move |_, value, _, _| {
            value.get::<ListBoxRow>().is_ok_and(|source_row| {
                let source_index = source_row.index();
                let target_index = target_row_clone.index();

                if source_index != target_index {
                    lb_clone2.remove(&source_row);
                    lb_clone2.insert(&source_row, target_index);

                    let new_sort = reconstruct_clone2(&lb_clone2, &om_clone2.borrow());
                    update_clone2(new_sort);
                }
                true
            })
        });
        row.add_controller(drop_target);

        list_box.append(&row);
    }

    list_box
}

/// Reconstructs album sort items from a `ListBox`'s current order.
///
/// # Arguments
///
/// * `list_box` - The `ListBox` containing sort rows
/// * `order_map` - Map of sort criteria to their current order
///
/// # Returns
///
/// A vector of `AlbumGridSortItem` with the current sort criteria and order
fn reconstruct_albums_sort(
    list_box: &ListBox,
    order_map: &HashMap<AlbumGridSortCriteria, SortOrder>,
) -> Vec<AlbumGridSortItem> {
    let mut new_sort = Vec::new();
    let mut child = list_box.first_child();
    while let Some(w) = &child {
        if let Ok(row) = w.clone().downcast::<ListBoxRow>() {
            let name = row.widget_name().to_string();
            let criteria = match name.as_str() {
                "Title" => Title,
                "Artist" => Artist,
                "Year" => Year,
                "DR" => DRValue,
                "Format" => Format,
                "Bit Depth" => BitDepth,
                "Sample Rate" => SampleRate,
                _ => {
                    error!("Unknown album sort criteria: {name}");
                    continue;
                }
            };
            let order = order_map
                .get(&criteria)
                .copied()
                .unwrap_or(SortOrder::default());
            new_sort.push(AlbumGridSortItem { criteria, order });
        }
        child = w.next_sibling();
    }
    new_sort
}

/// Reconstructs artist sort items from a `ListBox`'s current order.
///
/// # Arguments
///
/// * `list_box` - The `ListBox` containing sort rows
/// * `order_map` - Map of sort criteria to their current order
///
/// # Returns
///
/// A vector of `ArtistGridSortItem` with the current sort criteria and order
fn reconstruct_artists_sort(
    list_box: &ListBox,
    order_map: &HashMap<ArtistGridSortCriteria, SortOrder>,
) -> Vec<ArtistGridSortItem> {
    let mut new_sort = Vec::new();
    let mut child = list_box.first_child();
    while let Some(w) = &child {
        if let Ok(row) = w.clone().downcast::<ListBoxRow>() {
            let name = row.widget_name().to_string();
            let criteria = match name.as_str() {
                "Name" => Name,
                "Album Count" => AlbumCount,
                _ => {
                    error!("Unknown artist sort criteria: {name}");
                    continue;
                }
            };
            let order = order_map
                .get(&criteria)
                .copied()
                .unwrap_or(SortOrder::default());
            new_sort.push(ArtistGridSortItem { criteria, order });
        }
        child = w.next_sibling();
    }
    new_sort
}

/// Builds the `ListBox` for albums sort configuration.
///
/// # Arguments
///
/// * `app_state` - The application state
///
/// # Returns
///
/// A configured `ListBox` widget.
pub fn build_albums_sort_list(app_state: &Arc<AppState>) -> ListBox {
    let settings = app_state
        .get_settings_manager()
        .read()
        .get_settings()
        .clone();

    build_sort_list(settings.albums_grid_sort, reconstruct_albums_sort, {
        let as_clone = Arc::clone(app_state);
        move |sort| as_clone.update_albums_grid_sort(sort)
    })
}

/// Builds the `ListBox` for artists sort configuration.
///
/// # Arguments
///
/// * `app_state` - The application state
///
/// # Returns
///
/// A configured `ListBox` widget.
pub fn build_artists_sort_list(app_state: &Arc<AppState>) -> ListBox {
    let settings = app_state
        .get_settings_manager()
        .read()
        .get_settings()
        .clone();

    build_sort_list(settings.artists_grid_sort, reconstruct_artists_sort, {
        let as_clone = Arc::clone(app_state);
        move |sort| as_clone.update_artists_grid_sort(sort)
    })
}

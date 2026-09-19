//! Sort configuration list component with drag-and-drop reordering.
//!
//! Provides a draggable `ListBox` for configuring grid view sorting criteria.
//! Row-order reconstruction lives in the sibling [`reconstruct`] module.

pub mod reconstruct;

use std::{
    collections::HashMap,
    fmt::{Debug, Formatter, Result as FmtResult},
    hash::Hash,
    sync::Arc,
};

use {
    libadwaita::{
        glib::Value,
        gtk::{
            Align::{Center, Start},
            Box, Button, DragSource, DropTarget, Image, Label, ListBox, ListBoxRow,
            Orientation::Horizontal,
            SelectionMode::None,
            gdk::{ContentProvider, DragAction},
        },
        prelude::{BoxExt, ButtonExt, ListBoxRowExt, StaticType, ToValue, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    storage::sort_rules::{
        AlbumSortCriteria, AlbumSortItem, ArtistSortCriteria, ArtistSortItem,
        SortOrder::{self, Ascending, Descending},
    },
    ui::{
        drag::reconstruct::{reconstruct_albums_sort, reconstruct_artists_sort},
        signal_handlers::UiHandles,
    },
};

/// Implements `SortItem` trait for a sort item type.
macro_rules! impl_sort_item {
    ($ty:ty, $criteria:ty, $entity:expr) => {
        impl SortItem for $ty {
            type Criteria = $criteria;

            /// Returns the criteria.
            fn criteria(&self) -> &Self::Criteria {
                &self.criteria
            }

            /// Returns the order.
            fn order(&self) -> &SortOrder {
                &self.order
            }

            /// Construct from parts.
            fn new(criteria: Self::Criteria, order: SortOrder) -> Self {
                Self { criteria, order }
            }

            /// Parse a discriminator into criteria.
            fn from_discriminator(d: u8) -> Option<Self::Criteria> {
                <$criteria>::from_discriminator(d)
            }

            /// Human-readable entity name for error messages.
            fn entity_name() -> &'static str {
                $entity
            }

            /// Returns a numeric discriminator for the criteria.
            fn discriminator(&self) -> u8 {
                self.criteria.discriminator()
            }
        }
    };
}

impl_sort_item!(AlbumSortItem, AlbumSortCriteria, "album");
impl_sort_item!(ArtistSortItem, ArtistSortCriteria, "artist");

/// Trait for sort item types with criteria and order.
pub trait SortItem {
    /// The criteria type for this sort item.
    type Criteria: Clone + Hash + Eq + ToString;

    /// Returns the criteria.
    fn criteria(&self) -> &Self::Criteria;

    /// Returns the order.
    fn order(&self) -> &SortOrder;

    /// Construct from parts.
    fn new(criteria: Self::Criteria, order: SortOrder) -> Self;

    /// Parse a discriminator into criteria.
    fn from_discriminator(d: u8) -> Option<Self::Criteria>;

    /// Human-readable entity name for error messages.
    fn entity_name() -> &'static str;

    /// Returns a numeric discriminator for the criteria (used as a widget
    /// identifier, avoiding string‑based widget‑name parsing).
    fn discriminator(&self) -> u8;
}

/// Helper trait grouping the common bounds for sort list operations.
pub trait SortListBounds: SortItem
where
    Self::Criteria: Clone + Hash + Eq + ToString + 'static,
{
}

/// Builder for constructing a sort‑configuration `ListBox`.
///
/// # Type parameters
///
/// * `T` - The sort item type
/// * `F` - Callback type that rebuilds the sort vector from box order
/// * `U` - Callback type that persists a new sort vector
pub struct SortListBuilder<T: SortListBounds, F, U>
where
    T::Criteria: 'static,
{
    /// The `ListBox` being built.
    list_box: ListBox,
    /// Tracks the current order of each criteria.
    order_map: Arc<Mutex<HashMap<T::Criteria, SortOrder>>>,
    /// Callback to reconstruct the sort list from box order.
    reconstruct: F,
    /// Callback to persist the new sort order.
    update: U,
}

impl<T, F, U> SortListBuilder<T, F, U>
where
    T: SortListBounds,
    T::Criteria: 'static,
    F: Fn(&ListBox, &HashMap<T::Criteria, SortOrder>) -> Vec<T> + Clone + 'static,
    U: Fn(Vec<T>) + Clone + 'static,
{
    /// Add a single sort row to the list box.
    fn add_row(&self, item: &T) {
        let row = ListBoxRow::builder().css_classes(["sort-row"]).build();
        row.set_widget_name(&format!("sort:{}", item.discriminator()));

        let row_box = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();

        let drag_handle = Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["dim-label"])
            .build();
        row_box.append(&drag_handle);

        let label = Label::builder()
            .label(item.criteria().to_string())
            .halign(Start)
            .hexpand(true)
            .build();
        row_box.append(&label);

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
        let lb_clone = self.list_box.clone();
        let om_clone = Arc::clone(&self.order_map);
        let reconstruct_click = self.reconstruct.clone();
        let update_click = self.update.clone();

        let mut handles = UiHandles::default();
        handles.retain_signal(toggle.connect_clicked(move |_| {
            let mut om = om_clone.lock();
            let new_order = match om.get(&criteria_clone).unwrap_or(&Ascending) {
                Ascending => Descending,
                Descending => Ascending,
            };
            _ = om.insert(criteria_clone.clone(), new_order);
            toggle_clone.set_icon_name(match new_order {
                Ascending => "pan-up-symbolic",
                Descending => "pan-down-symbolic",
            });

            let new_sort = reconstruct_click(&lb_clone, &om);
            update_click(new_sort);
            drop(om);
        }));

        row_box.append(&toggle);
        row.set_child(Some(&row_box));

        let drag_source = DragSource::new();
        drag_source.set_actions(DragAction::MOVE);
        let row_clone = row.clone();
        handles.retain_signal(drag_source.connect_prepare(move |_, _, _| {
            Some(ContentProvider::for_value(&row_clone.to_value()))
        }));
        row.add_controller(drag_source);

        let drop_target = DropTarget::new(ListBoxRow::static_type(), DragAction::MOVE);
        drop_target.set_preload(true);
        let lb_clone2 = self.list_box.clone();
        let target_row_clone = row.clone();
        let om_clone2 = Arc::clone(&self.order_map);
        let reconstruct_drop = self.reconstruct.clone();
        let update_drop = self.update.clone();

        let lb = lb_clone2;
        let target = target_row_clone;
        let om = om_clone2;
        let reconstruct = reconstruct_drop;
        let update = update_drop;
        handles.retain_signal(drop_target.connect_drop(move |_, value, _, _| {
            Self::on_sort_row_drop(value, &lb, target.index(), &om, &reconstruct, &update)
        }));
        row.add_controller(drop_target);

        self.list_box.append(&row);
    }

    /// Handle a drop event for reordering sort rows.
    fn on_sort_row_drop(
        value: &Value,
        lb: &ListBox,
        target_idx: i32,
        om: &Mutex<HashMap<T::Criteria, SortOrder>>,
        reconstruct: &F,
        update: &U,
    ) -> bool
    where
        F: Fn(&ListBox, &HashMap<T::Criteria, SortOrder>) -> Vec<T>,
        U: Fn(Vec<T>),
    {
        let Ok(row) = value.get::<ListBoxRow>() else {
            return true;
        };
        let source_idx = row.index();
        if source_idx == target_idx {
            return true;
        }
        lb.remove(&row);
        lb.insert(&row, adjust_insert_index(source_idx, target_idx));
        let om_guard = om.lock();
        let new_sort = reconstruct(lb, &om_guard);
        update(new_sort);
        drop(om_guard);
        true
    }

    /// Build a sort‑configuration list box from the given items.
    ///
    /// # Arguments
    ///
    /// * `sort_items` - The sort items to render as rows
    /// * `reconstruct` - Callback that rebuilds the sort vector from box order
    /// * `update` - Callback that persists a new sort vector
    pub fn build(sort_items: &[T], reconstruct: F, update: U) -> ListBox {
        let list_box = ListBox::builder()
            .selection_mode(None)
            .css_classes(["boxed-list", "sort-list"])
            .build();

        let order_map: Arc<Mutex<HashMap<T::Criteria, SortOrder>>> =
            Arc::new(Mutex::new(HashMap::with_capacity(sort_items.len())));
        for item in sort_items {
            let mut om = order_map.lock();
            _ = om.insert(item.criteria().clone(), *item.order());
        }

        let builder = Self {
            list_box,
            order_map,
            reconstruct,
            update,
        };
        for item in sort_items {
            builder.add_row(item);
        }

        builder.list_box
    }
}

impl<T, F, U> Debug for SortListBuilder<T, F, U>
where
    T: SortListBounds,
    T::Criteria: 'static,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("SortListBuilder").finish_non_exhaustive()
    }
}

impl<T: SortItem> SortListBounds for T where T::Criteria: Clone + Hash + Eq + ToString + 'static {}

/// Compute the insertion index for `source_idx` after it has been removed,
/// so the row lands on `target_idx`.
///
/// Removing a source row above the target shifts the target down by one,
/// so the insertion index must be decremented in that direction; dragging
/// downward needs no adjustment.
const fn adjust_insert_index(source_idx: i32, target_idx: i32) -> i32 {
    if source_idx < target_idx {
        target_idx.saturating_sub(1)
    } else {
        target_idx
    }
}

/// Apply album sort configuration to memory, notify listeners, and
/// schedule a debounced disk write.
fn save_albums_sort(sort: Vec<AlbumSortItem>, state: &Arc<AppState>) {
    state.storage.set_albums_sort_memory(sort);
    if let Err(e) = state.albums_sort_tx.try_send(()) {
        error!(error = %e, "Failed to send album sort config change");
    }
    state.storage.save_settings();
}

/// Apply artist sort configuration to memory, notify listeners, and
/// schedule a debounced disk write.
fn save_artists_sort(sort: Vec<ArtistSortItem>, state: &Arc<AppState>) {
    state.storage.set_artists_sort_memory(sort);
    if let Err(e) = state.artists_sort_tx.try_send(()) {
        error!(error = %e, "Failed to send artist sort config change");
    }
    state.storage.save_settings();
}

/// Builds the `ListBox` for albums sort configuration.
pub fn build_albums_drag_list(state: &Arc<AppState>) -> ListBox {
    let sort_items = state.storage.get_albums_sort();

    SortListBuilder::build(&sort_items, reconstruct_albums_sort, {
        let state_clone = Arc::clone(state);
        move |sort| save_albums_sort(sort, &state_clone)
    })
}

/// Builds the `ListBox` for artists sort configuration.
pub fn build_artists_drag_list(state: &Arc<AppState>) -> ListBox {
    let sort_items = state.storage.get_artists_sort();

    SortListBuilder::build(&sort_items, reconstruct_artists_sort, {
        let state_clone = Arc::clone(state);
        move |sort| save_artists_sort(sort, &state_clone)
    })
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::drag::adjust_insert_index;

    #[test]
    fn downward_drag_inserts_before_target() -> Result<()> {
        ensure!(adjust_insert_index(0, 3) == 2);
        ensure!(adjust_insert_index(2, 4) == 3);
        Ok(())
    }

    #[test]
    fn upward_drag_inserts_before_target() -> Result<()> {
        ensure!(adjust_insert_index(4, 2) == 2);
        ensure!(adjust_insert_index(3, 0) == 0);
        Ok(())
    }

    #[test]
    fn same_position_is_a_noop() -> Result<()> {
        ensure!(adjust_insert_index(1, 1) == 1);
        ensure!(adjust_insert_index(2, 2) == 2);
        Ok(())
    }
}

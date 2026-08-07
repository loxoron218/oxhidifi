//! Sort configuration list component.
//!
//! Provides a drag-and-drop sortable list for configuring grid view sorting criteria.

use std::{collections::HashMap, hash::Hash, sync::Arc};

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
        prelude::{BoxExt, ButtonExt, Cast, ListBoxRowExt, StaticType, ToValue, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::{error, warn},
};

use crate::{
    app::AppState,
    storage::sort_rules::{
        AlbumSortCriteria, AlbumSortItem, ArtistSortCriteria, ArtistSortItem,
        SortOrder::{self, Ascending, Descending},
    },
};

/// Implements `SortItem` trait for a sort item type.
macro_rules! impl_sort_item {
    ($ty:ty, $criteria:ty, $entity:expr) => {
        impl SortItem for $ty {
            type Criteria = $criteria;

            fn criteria(&self) -> &Self::Criteria {
                &self.criteria
            }

            fn order(&self) -> &SortOrder {
                &self.order
            }

            fn new(criteria: Self::Criteria, order: SortOrder) -> Self {
                Self { criteria, order }
            }

            fn from_discriminator(d: u8) -> Option<Self::Criteria> {
                <$criteria>::from_discriminator(d)
            }

            fn entity_name() -> &'static str {
                $entity
            }

            fn discriminator(&self) -> u8 {
                self.criteria.discriminator()
            }
        }
    };
}

impl_sort_item!(AlbumSortItem, AlbumSortCriteria, "album");
impl_sort_item!(ArtistSortItem, ArtistSortCriteria, "artist");

/// Trait for sort item types with criteria and order.
trait SortItem {
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
trait SortListBounds: SortItem
where
    Self::Criteria: Clone + Hash + Eq + ToString + 'static,
{
}

/// Internal builder for constructing a sort‑configuration `ListBox`.
struct SortListBuilder<T: SortListBounds, F, U>
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

        toggle.connect_clicked(move |_| {
            let mut om = om_clone.lock();
            let new_order = match om.get(&criteria_clone).unwrap_or(&Ascending) {
                Ascending => Descending,
                Descending => Ascending,
            };
            om.insert(criteria_clone.clone(), new_order);
            toggle_clone.set_icon_name(match new_order {
                Ascending => "pan-up-symbolic",
                Descending => "pan-down-symbolic",
            });

            let new_sort = reconstruct_click(&lb_clone, &om);
            update_click(new_sort);
            drop(om);
        });

        row_box.append(&toggle);
        row.set_child(Some(&row_box));

        let drag_source = DragSource::new();
        drag_source.set_actions(DragAction::MOVE);
        let row_clone = row.clone();
        drag_source.connect_prepare(move |_, _, _| {
            Some(ContentProvider::for_value(&row_clone.to_value()))
        });
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
        drop_target.connect_drop(move |_, value, _, _| {
            Self::on_sort_row_drop(value, &lb, target.index(), &om, &reconstruct, &update)
        });
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

    /// Build the sort‑configuration list box from the given items.
    fn build(sort_items: &[T], reconstruct: F, update: U) -> ListBox {
        let list_box = ListBox::builder()
            .selection_mode(None)
            .css_classes(["boxed-list", "sort-list"])
            .build();

        let order_map: Arc<Mutex<HashMap<T::Criteria, SortOrder>>> =
            Arc::new(Mutex::new(HashMap::with_capacity(sort_items.len())));
        for item in sort_items {
            let mut om = order_map.lock();
            om.insert(item.criteria().clone(), *item.order());
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

impl<T: SortItem> SortListBounds for T where T::Criteria: Clone + Hash + Eq + ToString + 'static {}

/// Compute the insertion index for `source_idx` after it has been removed,
/// so the row lands on `target_idx`.
///
/// Removing a source row above the target shifts the target down by one,
/// so the insertion index must be decremented in that direction; dragging
/// downward needs no adjustment.
#[must_use]
fn adjust_insert_index(source_idx: i32, target_idx: i32) -> i32 {
    if source_idx < target_idx {
        target_idx - 1
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
pub fn build_albums_sort_list(state: &Arc<AppState>) -> ListBox {
    let sort_items = state.storage.get_albums_sort();

    SortListBuilder::build(&sort_items, reconstruct_albums_sort, {
        let state_clone = Arc::clone(state);
        move |sort| save_albums_sort(sort, &state_clone)
    })
}

/// Builds the `ListBox` for artists sort configuration.
pub fn build_artists_sort_list(state: &Arc<AppState>) -> ListBox {
    let sort_items = state.storage.get_artists_sort();

    SortListBuilder::build(&sort_items, reconstruct_artists_sort, {
        let state_clone = Arc::clone(state);
        move |sort| save_artists_sort(sort, &state_clone)
    })
}

/// Parse a `"sort:N"` widget name into a sort item and push to `new_sort`.
fn process_row<T: SortListBounds>(
    row: &ListBoxRow,
    order_map: &HashMap<T::Criteria, SortOrder>,
    new_sort: &mut Vec<T>,
) where
    T::Criteria: 'static,
{
    let name = row.widget_name();
    let Some(suffix) = name.strip_prefix("sort:") else {
        warn!(name = %name, "Invalid sort row name");
        return;
    };
    let disc = match suffix.parse::<u8>() {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, suffix = %suffix, "Failed to parse sort discriminator");
            return;
        }
    };
    let Some(criteria) = T::from_discriminator(disc) else {
        warn!(entity = %T::entity_name(), disc = %disc, "Unknown sort discriminator");
        return;
    };
    let order = order_map
        .get(&criteria)
        .copied()
        .unwrap_or(SortOrder::default());
    new_sort.push(T::new(criteria, order));
}

/// Reconstruct sort items from a `ListBox`'s current order using a processing callback.
fn reconstruct_sort_list<C, I>(
    list_box: &ListBox,
    order_map: &HashMap<C, SortOrder>,
    mut process: impl FnMut(&ListBoxRow, &HashMap<C, SortOrder>, &mut Vec<I>),
) -> Vec<I> {
    let mut result = Vec::new();
    let mut child = list_box.first_child();
    while let Some(w) = &child {
        if let Ok(row) = w.clone().downcast::<ListBoxRow>() {
            process(&row, order_map, &mut result);
        }
        child = w.next_sibling();
    }
    result
}

/// Reconstructs album sort items from a `ListBox`'s current order.
fn reconstruct_albums_sort(
    list_box: &ListBox,
    order_map: &HashMap<AlbumSortCriteria, SortOrder>,
) -> Vec<AlbumSortItem> {
    reconstruct_sort_list(list_box, order_map, process_row::<AlbumSortItem>)
}

/// Reconstructs artist sort items from a `ListBox`'s current order.
fn reconstruct_artists_sort(
    list_box: &ListBox,
    order_map: &HashMap<ArtistSortCriteria, SortOrder>,
) -> Vec<ArtistSortItem> {
    reconstruct_sort_list(list_box, order_map, process_row::<ArtistSortItem>)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, ListBox, ListBoxRow, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        storage::sort_rules::{
            AlbumSortCriteria::{Artist, BitDepth, Title, Year},
            ArtistSortCriteria::{AlbumCount, Name},
            SortOrder::{Ascending, Descending},
        },
        ui::sort_list::{adjust_insert_index, reconstruct_albums_sort, reconstruct_artists_sort},
    };

    fn row_named(name: &str) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.set_widget_name(name);
        row
    }

    fn albums_list_box(names: &[&str]) -> ListBox {
        let list_box = ListBox::new();
        for name in names {
            list_box.append(&row_named(name));
        }
        list_box
    }

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

    #[test]
    fn reconstruct_albums_sort_preserves_box_order() -> Result<()> {
        let list_box = albums_list_box(&["sort:0", "sort:4", "sort:2"]);
        let mut order_map = HashMap::new();
        order_map.insert(Title, Descending);
        order_map.insert(BitDepth, Ascending);
        order_map.insert(Year, Ascending);

        let sort = reconstruct_albums_sort(&list_box, &order_map);
        let criteria: Vec<_> = sort.iter().map(|item| item.criteria.clone()).collect();
        ensure!(
            criteria == vec![Title, BitDepth, Year],
            "reconstructed order must follow the box order"
        );
        ensure!(sort[0].order == Descending, "order must come from the map");
        ensure!(sort[1].order == Ascending, "order must come from the map");
        Ok(())
    }

    #[test]
    fn reconstruct_artists_sort_preserves_box_order() -> Result<()> {
        let list_box = ListBox::new();
        list_box.append(&row_named("sort:1"));
        list_box.append(&row_named("sort:0"));
        let mut order_map = HashMap::new();
        order_map.insert(AlbumCount, Descending);
        order_map.insert(Name, Ascending);

        let sort = reconstruct_artists_sort(&list_box, &order_map);
        let criteria: Vec<_> = sort.iter().map(|item| item.criteria.clone()).collect();
        ensure!(
            criteria == vec![AlbumCount, Name],
            "reconstructed order must follow the box order"
        );
        ensure!(sort[0].order == Descending, "order must come from the map");
        ensure!(sort[1].order == Ascending, "order must come from the map");
        Ok(())
    }

    #[test]
    fn reconstruct_skips_invalid_and_unknown_row_names() -> Result<()> {
        let list_box = albums_list_box(&["not-a-sort-row", "sort:99", "sort:1"]);
        let order_map = HashMap::new();

        let sort = reconstruct_albums_sort(&list_box, &order_map);
        ensure!(
            sort.len() == 1,
            "only the valid known row must survive reconstruction"
        );
        ensure!(
            sort[0].criteria == Artist,
            "the surviving row must decode to its criteria"
        );
        Ok(())
    }

    #[test]
    fn reconstruct_defaults_missing_order_to_ascending() -> Result<()> {
        let list_box = albums_list_box(&["sort:0"]);
        let order_map = HashMap::new();

        let sort = reconstruct_albums_sort(&list_box, &order_map);
        ensure!(sort.len() == 1, "a known criteria must be reconstructed");
        ensure!(
            sort[0].order == Ascending,
            "a criteria missing from the order map must default to ascending"
        );
        Ok(())
    }
}

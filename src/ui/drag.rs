//! Sort configuration list component with drag-and-drop reordering.
//!
//! Provides a draggable `ListBox` for configuring grid view sorting criteria.
//! Row-order reconstruction lives in the sibling [`reconstruct`] module.
//! Row construction and drop reordering live in the sibling [`sort_rows`] module.

pub mod reconstruct;
pub mod sort_rows;

use std::{
    collections::HashMap,
    fmt::{Debug, Formatter, Result as FmtResult},
    hash::Hash,
    sync::Arc,
};

use {libadwaita::gtk::ListBox, parking_lot::Mutex, tracing::error};

use crate::{
    app::runtime::AppState,
    storage::sort_rules::{
        AlbumSortCriteria, AlbumSortItem, ArtistSortCriteria, ArtistSortItem, SortOrder,
    },
    ui::drag::reconstruct::{reconstruct_albums_sort, reconstruct_artists_sort},
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

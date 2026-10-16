//! Shared artist-page selection sync across album sections.
//!
//! Clearing sibling track-list highlights keeps a single active highlight
//! across album sections. Used by the data loader in
//! [`artist_load`](crate::ui::detail::artist_load); defined here so the loader
//! and the page share it without mutual imports.

use libadwaita::gtk::{ListBox, ListBoxRow};

/// When a row is selected in one album's track list, unselect all rows
/// in the other albums' track lists to keep a single active highlight.
///
/// # Arguments
///
/// * `row` - Newly selected row, or `None` when the selection was cleared.
/// * `others` - Track lists belonging to sibling album sections.
pub fn clear_other_lists(row: Option<&ListBoxRow>, others: &[ListBox]) {
    if row.is_none() {
        return;
    }
    for other in others {
        other.unselect_all();
    }
}

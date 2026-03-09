//! Album column sorter utilities.

use std::sync::Arc;

use libadwaita::{
    glib::{BoxedAnyObject, Object},
    gtk::{
        CustomSorter,
        Ordering::{self, Equal, Larger, Smaller},
    },
    prelude::Cast,
};

use crate::library::models::Album;

/// Extracts an album from a generic object.
///
/// # Arguments
///
/// * `item` - The object to extract the album from
///
/// # Returns
///
/// The album if it can be extracted
fn extract_album(item: &Object) -> Option<Arc<Album>> {
    item.downcast_ref::<BoxedAnyObject>().map(|boxed| {
        let album_ref = boxed.borrow::<Arc<Album>>();
        Arc::clone(&album_ref)
    })
}

/// Creates a string-based sorter for album columns.
///
/// # Arguments
///
/// * `get_value` - Function to extract the string value from an album
///
/// # Returns
///
/// A `CustomSorter` for sorting albums by string values
pub fn create_string_sorter(get_value: fn(&Album) -> Option<&String>) -> CustomSorter {
    CustomSorter::new(move |item1, item2| {
        let Some(arc_album1) = extract_album(item1) else {
            return Equal;
        };
        let Some(arc_album2) = extract_album(item2) else {
            return Equal;
        };

        let val1 = get_value(&arc_album1);
        let val2 = get_value(&arc_album2);

        match (val1, val2) {
            (Some(s1), Some(s2)) => {
                Ordering::from(s1.to_ascii_lowercase().cmp(&s2.to_ascii_lowercase()))
            }
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => Equal,
        }
    })
}

/// Creates a numeric-based sorter for album columns.
///
/// # Arguments
///
/// * `get_value` - Function to extract the numeric value from an album
///
/// # Returns
///
/// A `CustomSorter` for sorting albums by numeric values
pub fn create_numeric_sorter(get_value: fn(&Album) -> Option<i64>) -> CustomSorter {
    CustomSorter::new(move |item1, item2| {
        let Some(arc_album1) = extract_album(item1) else {
            return Equal;
        };
        let Some(arc_album2) = extract_album(item2) else {
            return Equal;
        };

        let val1 = get_value(&arc_album1);
        let val2 = get_value(&arc_album2);

        match (val1, val2) {
            (Some(n1), Some(n2)) => Ordering::from(n1.cmp(&n2)),
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => Equal,
        }
    })
}

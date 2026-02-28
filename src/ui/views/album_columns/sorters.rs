//! Album column sorter utilities.

use libadwaita::{
    glib::BoxedAnyObject,
    gtk::{
        CustomSorter,
        Ordering::{self, Equal, Larger, Smaller},
    },
    prelude::Cast,
};

use crate::library::models::Album;

/// Creates a string-based sorter for album columns.
///
/// # Arguments
///
/// * `get_value` - Function to extract the string value from an album
///
/// # Returns
///
/// A `CustomSorter` for sorting albums by string values
pub fn create_string_sorter(get_value: fn(&Album) -> Option<String>) -> CustomSorter {
    CustomSorter::new(move |item1, item2| {
        let val1 = item1.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
            let album = boxed.borrow::<Album>();
            get_value(&album)
        });

        let val2 = item2.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
            let album = boxed.borrow::<Album>();
            get_value(&album)
        });

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
        let val1 = item1.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
            let album = boxed.borrow::<Album>();
            get_value(&album)
        });

        let val2 = item2.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
            let album = boxed.borrow::<Album>();
            get_value(&album)
        });

        match (val1, val2) {
            (Some(n1), Some(n2)) => Ordering::from(n1.cmp(&n2)),
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => Equal,
        }
    })
}

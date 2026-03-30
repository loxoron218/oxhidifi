//! Sorting utility functions.

use std::cmp::Ordering::{Equal, Greater, Less};

use libadwaita::gtk::Ordering::{self as GtkOrdering, Equal as GtkEqual, Larger, Smaller};

/// Performs case-insensitive ASCII string comparison.
///
/// This is used for GTK column sorting where we need consistent
/// case-insensitive ordering.
///
/// # Arguments
///
/// * `s1` - First string to compare
/// * `s2` - Second string to compare
///
/// # Returns
///
/// `Ordering::Equal` if strings are equal, `Smaller` if `s1 < s2`, `Larger` if `s1 > s2`
#[must_use]
pub fn compare_ignore_ascii_case(s1: &str, s2: &str) -> GtkOrdering {
    let bytes1 = s1.as_bytes();
    let bytes2 = s2.as_bytes();

    let len = bytes1.len().min(bytes2.len());

    for i in 0..len {
        let b1 = bytes1[i];
        let b2 = bytes2[i];

        let c1 = if b1.is_ascii_uppercase() { b1 + 32 } else { b1 };
        let c2 = if b2.is_ascii_uppercase() { b2 + 32 } else { b2 };

        if c1 != c2 {
            return if c1 < c2 { Smaller } else { Larger };
        }
    }

    match bytes1.len().cmp(&bytes2.len()) {
        Equal => GtkEqual,
        Less => Smaller,
        Greater => Larger,
    }
}

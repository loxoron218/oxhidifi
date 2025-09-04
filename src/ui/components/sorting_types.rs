use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::ui::components::sorting_types::SortOrder::{Album, Artist, DrValue, Year};

/// Represents the sorting order for library views.
///
/// This `enum` defines the various criteria by which albums or artists can be sorted
/// within the application's library views. It supports sorting by `Artist`, `Album` title,
/// `Year` of release, and audio `Format`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum SortOrder {
    Artist,
    Album,
    Year,
    DrValue,
}

/// Allows parsing a `SortOrder` from a string ("Artist", "Year", etc). Useful for persistence and drag-and-drop.
///
/// This `impl FromStr for SortOrder` enables conversion from string representations
/// (e.g., read from a configuration file or a drag-and-drop operation) into the
/// `SortOrder` enum variants. This is crucial for persisting user preferences
/// and handling UI interactions.
impl FromStr for SortOrder {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Artist" => Ok(Artist),
            "Year" => Ok(Year),
            "Album" => Ok(Album),
            "DR Value" => Ok(DrValue),
            _ => Err(()),
        }
    }
}

/// Returns the display label for a given `SortOrder` variant (for UI).
///
/// This helper function provides a human-readable string representation for each
/// `SortOrder` enum variant, suitable for display in the user interface (e.g.,
/// "Artist" for `SortOrder::Artist`).
///
/// # Arguments
///
/// * `order` - A reference to the `SortOrder` variant.
///
/// # Returns
///
/// A `&'static str` containing the display label.
pub fn sort_order_label(order: &SortOrder) -> &'static str {
    match order {
        Artist => "Artist",
        Year => "Year",
        Album => "Album",
        DrValue => "DR Value",
    }
}

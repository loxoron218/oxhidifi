use std::{cell::Cell, rc::Rc};

/// Determines the appropriate sort icon name based on the current page and sort order.
///
/// This helper function returns the symbolic icon name for the sort button,
/// choosing between "view-sort-descending-symbolic" and "view-sort-ascending-symbolic"
/// based on the `page` (e.g., "artists" or "albums") and the corresponding
/// `sort_ascending` state for that page.
///
/// # Arguments
///
/// * `page` - A string slice representing the current visible page name (e.g., "artists", "albums").
/// * `sort_ascending` - A reference to an `Rc<Cell<bool>>` indicating the sort direction for albums.
/// * `sort_ascending_artists` - A reference to an `Rc<Cell<bool>>` indicating the sort direction for artists.
///
/// # Returns
///
/// A `&'static str` containing the appropriate symbolic icon name.
pub fn get_sort_icon_name(
    page: &str,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
) -> &'static str {
    // 1. Select the correct boolean value based on the page.
    let ascending = if page == "artists" {
        sort_ascending_artists.get()
    } else {
        sort_ascending.get()
    };

    // 2. Use that boolean in the now-deduplicated logic.
    if ascending {
        "view-sort-descending-symbolic"
    } else {
        "view-sort-ascending-symbolic"
    }
}

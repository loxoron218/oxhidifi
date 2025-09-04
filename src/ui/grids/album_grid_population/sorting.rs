use std::{cell::RefCell, cmp::Ordering::Equal, rc::Rc};

use crate::ui::{
    components::sorting_types::SortOrder::{self, Album, Artist, DrValue, Year},
    grids::album_grid_state::AlbumGridItem,
};

/// Sorts albums according to the specified sort orders and direction.
///
/// This function performs a multi-level sort on the albums based on user-defined criteria.
///
/// # Arguments
/// * `albums` - A mutable reference to a vector of `AlbumGridItem` to sort
/// * `sort_orders` - A `Rc<RefCell<Vec<SortOrder>>>` defining the multi-level sorting criteria
/// * `sort_ascending` - A boolean indicating the overall sort direction (ascending/descending)
pub fn sort_albums(
    albums: &mut Vec<AlbumGridItem>,
    sort_orders: &Rc<RefCell<Vec<SortOrder>>>,
    sort_ascending: bool,
) {
    let current_sort_orders = sort_orders.borrow();
    albums.sort_by(|a, b| {
        for order in &*current_sort_orders {
            let cmp = match order {
                Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                Album => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                Year => {
                    // Extract year from original_release_date or year field.
                    let a_year = a
                        .original_release_date
                        .as_ref()
                        .and_then(|s| s.split('-').next())
                        .and_then(|y| y.parse::<i32>().ok())
                        .or(a.year);
                    let b_year = b
                        .original_release_date
                        .as_ref()
                        .and_then(|s| s.split('-').next())
                        .and_then(|y| y.parse::<i32>().ok())
                        .or(b.year);
                    a_year.cmp(&b_year)
                }
                DrValue => a.dr_value.cmp(&b.dr_value),
            };

            // If comparison is not equal, return the result, applying ascending/descending.
            if cmp != Equal {
                return if sort_ascending { cmp } else { cmp.reverse() };
            }
        }

        // If all sort criteria are equal, maintain original order (or arbitrary).
        Equal
    });
}

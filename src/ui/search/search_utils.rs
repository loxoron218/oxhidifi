use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use glib::{SourceId, source::timeout_add_local_once};
use gtk4::FlowBox;
use libadwaita::prelude::WidgetExt;

use crate::ui::grids::album_grid_state::AlbumGridItem;

// This import is needed for the sort_albums_by_relevance function
// use crate::ui::grids::album_grid_state::AlbumGridItem;

/// Clears all children from a given `FlowBox`.
///
/// This is a utility function to remove all visible items from a GTK `FlowBox`
/// widget, typically used before repopulating it with new content.
///
/// # Arguments
/// * `grid` - A reference to the `FlowBox` to be cleared.
pub fn clear_grid(grid: &FlowBox) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
}

/// Creates a debounced search timer that executes the provided closure after a delay.
///
/// This function manages search debouncing by cancelling any existing timer and
/// scheduling a new one. It's used to prevent excessive database queries while
/// the user is typing in the search field.
///
/// # Arguments
/// * `search_timer` - A reference to the `Rc<RefCell<Option<SourceId>>>` that tracks the current timer
/// * `delay` - The duration to wait before executing the closure
/// * `closure` - The closure to execute when the timer fires
pub fn debounce_search<F>(search_timer: &Rc<RefCell<Option<SourceId>>>, delay: Duration, closure: F)
where
    F: FnOnce() + 'static,
{
    // If a timer is already scheduled, cancel it by removing its source ID.
    if let Some(source_id) = search_timer.borrow_mut().take() {
        source_id.remove();
    }

    // Schedule a new timer.
    let search_timer_cloned = search_timer.clone();
    let source_id = timeout_add_local_once(delay, move || {
        // When the timer fires, its SourceId becomes invalid. By `take()`-ing it here,
        // we prevent the `connect_changed` handler from trying to `remove()` an invalid ID
        // if another key is pressed after the timer has already fired.
        search_timer_cloned.borrow_mut().take();

        closure();
    });

    // Store the ID of the new timer.
    *search_timer.borrow_mut() = Some(source_id);
}
/// Sorts albums based on relevance to the search query and then by artist/title.
///
/// This function implements a scoring system where:
/// - Exact match = 0
/// - Starts with query = 1
/// - Contains query = 2
/// - No match = 3
///
/// Albums are then sorted by score, then by artist name (ascending/descending based on sort_ascending).
///
/// # Arguments
/// * `albums` - A mutable reference to the vector of albums to sort
/// * `text` - The search query text
/// * `sort_ascending` - A reference to the Cell<bool> indicating sort direction
pub fn sort_albums_by_relevance(
    albums: &mut Vec<AlbumGridItem>,
    text: &str,
    sort_ascending: &Rc<Cell<bool>>,
) {
    albums.sort_by(|a, b| {
        let a_title = a.title.to_lowercase();
        let b_title = b.title.to_lowercase();
        let a_artist = a.artist.to_lowercase();
        let b_artist = b.artist.to_lowercase();
        let query = text.to_lowercase();

        // Scoring function: exact match (0), starts with (1), contains (2), no match (3)
        let score = |s: &str| {
            if s == query {
                0
            } else if s.starts_with(&query) {
                1
            } else if s.contains(&query) {
                2
            } else {
                3
            }
        };
        let a_score = score(&a_title).min(score(&a_artist));
        let b_score = score(&b_title).min(score(&b_artist));

        // Primary sort by score, then by artist name (ascending/descending)
        a_score.cmp(&b_score).then_with(|| {
            let cmp = a_artist.cmp(&b_artist);
            if sort_ascending.get() {
                cmp
            } else {
                cmp.reverse()
            }
        })
    });
}

//! Shared filtering trait for views.
//!
//! This module provides a generic trait for filtering items in views,
//! reducing code duplication across different view implementations.

use std::collections::HashSet;

/// Trait for filtering items in a view.
///
/// This trait provides a common interface for filtering operations
/// across different view types (albums, artists, tracks, etc.).
///
/// # Type Parameters
///
/// * `T` - The type of item to filter (e.g., `Album`, `Artist`), must implement `Clone`
///
/// # Example
///
/// ```ignore
/// impl Filterable<Album> for AlbumGridView {
///     fn get_widget_id(&self, item: &Album) -> i64 {
///         item.id
///     }
///
///     fn get_current_items(&self) -> Vec<Album> {
///         self.albums.clone()
///     }
///
///     fn set_current_items(&mut self, items: Vec<Album>) {
///         self.albums = items;
///     }
///
///     fn set_visibility(&self, visible_ids: &HashSet<i64>) {
///         let cards = self.album_cards.borrow();
///         for card in cards.iter() {
///             let card_visible = visible_ids.contains(&card.album_id);
///             card.widget.set_visible(card_visible);
///         }
///     }
/// }
///
/// // Then use it like:
/// view.filter_items("search query", &all_albums, |album, query| {
///     album.title.to_lowercase().contains(query)
///         || album.artist_id.to_string().to_lowercase().contains(query)
/// });
/// ```
pub trait Filterable<T: Clone> {
    /// Gets the unique identifier for an item.
    ///
    /// # Arguments
    ///
    /// * `item` - The item to get the ID from
    ///
    /// # Returns
    ///
    /// The unique ID of the item
    fn get_widget_id(&self, item: &T) -> i64;

    /// Gets the currently filtered items.
    ///
    /// # Returns
    ///
    /// A clone of the current items vector
    fn get_current_items(&self) -> Vec<T>;

    /// Sets the current items.
    ///
    /// # Arguments
    ///
    /// * `items` - New items to set
    fn set_current_items(&mut self, items: Vec<T>);

    /// Sets visibility of widgets based on item IDs.
    ///
    /// # Arguments
    ///
    /// * `visible_ids` - Set of IDs that should be visible
    fn set_visibility(&self, visible_ids: &HashSet<i64>);

    /// Clears the view by hiding all items.
    ///
    /// This is used when switching tabs with an active search to prevent
    /// the unfiltered view from appearing during the transition.
    fn clear_view(&self) {
        let empty_ids = HashSet::new();
        self.set_visibility(&empty_ids);
    }

    /// Filters items based on a search query and matching function.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    /// * `all_items` - Complete list of all items to filter from
    /// * `matches` - Function that determines if an item matches the query
    ///
    /// # Returns
    ///
    /// `true` if any items matched the query, `false` otherwise
    ///
    /// # Example
    ///
    /// ```ignore
    /// let has_results = view.filter_items(
    ///     "pink floyd",
    ///     &all_albums,
    ///     |album, query| {
    ///         album.title.to_lowercase().contains(query)
    ///             || album.artist_id.to_string().to_lowercase().contains(query)
    ///     }
    /// );
    /// ```
    fn filter_items(
        &mut self,
        query: &str,
        all_items: &[T],
        matches: impl Fn(&T, &str) -> bool,
    ) -> bool {
        let query_lower = query.to_lowercase();

        let filtered_ids: HashSet<i64> = all_items
            .iter()
            .filter(|item| matches(item, &query_lower))
            .map(|item| self.get_widget_id(item))
            .collect();

        let has_results = !filtered_ids.is_empty();

        let current_ids: HashSet<i64> = self
            .get_current_items()
            .iter()
            .map(|item| self.get_widget_id(item))
            .collect();

        let filter_unchanged = current_ids == filtered_ids;

        if !filter_unchanged {
            let filtered_items: Vec<T> = all_items
                .iter()
                .filter(|item| filtered_ids.contains(&self.get_widget_id(item)))
                .cloned()
                .collect();

            self.set_current_items(filtered_items);
            self.set_visibility(&filtered_ids);
        }

        has_results
    }
}

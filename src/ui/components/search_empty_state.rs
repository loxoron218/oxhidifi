//! Empty search result state component using `StatusPage`.
//!
//! This module implements `SearchEmptyState` component that displays
//! a user-friendly message when search returns no results, using
//! Libadwaita's `StatusPage` widget per GNOME HIG.

use libadwaita::{StatusPage, prelude::WidgetExt};

/// Configuration for `SearchEmptyState` display options.
#[derive(Debug, Clone, Default)]
pub struct SearchEmptyStateConfig {
    /// Whether this is for albums or artists search.
    pub is_album_view: bool,
}

/// Empty search result state UI component.
///
/// The `SearchEmptyState` component displays a clear message when
/// search returns no results using Libadwaita's `StatusPage` widget.
pub struct SearchEmptyState {
    /// The underlying `StatusPage` widget.
    widget: StatusPage,
    /// Current configuration.
    pub config: SearchEmptyStateConfig,
}

impl SearchEmptyState {
    /// Creates a new `SearchEmptyState` component.
    ///
    /// # Arguments
    ///
    /// * `config` - Display configuration
    ///
    /// # Returns
    ///
    /// A new `SearchEmptyState` instance.
    #[must_use]
    pub fn new(config: SearchEmptyStateConfig) -> Self {
        let status_page = StatusPage::builder()
            .icon_name("system-search-symbolic")
            .css_classes(["search-empty-state"])
            .build();

        Self {
            widget: status_page,
            config,
        }
    }

    /// Updates the empty state message based on search query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query that returned no results
    pub fn update_search_query(&self, query: &str) {
        let item_type = if self.config.is_album_view {
            "albums"
        } else {
            "artists"
        };

        let (title, description) = if query.is_empty() {
            (
                format!("No {item_type} available"),
                "Try searching for something",
            )
        } else {
            (
                format!("No {item_type} found for \"{query}\""),
                "Try searching for something else",
            )
        };

        self.widget.set_title(&title);
        self.widget.set_description(Some(description));
    }

    /// Shows the empty search state.
    pub fn show(&self) {
        self.widget.set_visible(true);
    }

    /// Hides the empty search state.
    pub fn hide(&self) {
        self.widget.set_visible(false);
    }

    /// Returns a reference to the underlying `StatusPage` widget.
    ///
    /// # Returns
    ///
    /// A reference to the `StatusPage` widget.
    #[must_use]
    pub fn widget(&self) -> &StatusPage {
        &self.widget
    }

    /// Creates a builder for configuring the search empty state.
    #[must_use]
    pub fn builder() -> SearchEmptyStateBuilder {
        SearchEmptyStateBuilder::default()
    }
}

impl Default for SearchEmptyState {
    fn default() -> Self {
        Self::new(SearchEmptyStateConfig {
            is_album_view: true,
        })
    }
}

/// Builder pattern for configuring `SearchEmptyState` components.
#[derive(Debug, Default)]
pub struct SearchEmptyStateBuilder {
    /// Configuration for the empty state.
    config: SearchEmptyStateConfig,
}

impl SearchEmptyStateBuilder {
    /// Sets whether this is for albums or artists.
    ///
    /// # Arguments
    ///
    /// * `is_album_view` - Whether this is for albums
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn is_album_view(mut self, is_album_view: bool) -> Self {
        self.config = SearchEmptyStateConfig { is_album_view };
        self
    }

    /// Builds the `SearchEmptyState` component.
    ///
    /// # Returns
    ///
    /// A new `SearchEmptyState` instance.
    #[must_use]
    pub fn build(self) -> SearchEmptyState {
        SearchEmptyState::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::components::{SearchEmptyState, SearchEmptyStateConfig};

    #[test]
    fn test_search_empty_state_config() {
        let album_config = SearchEmptyStateConfig {
            is_album_view: true,
        };
        let artist_config = SearchEmptyStateConfig {
            is_album_view: false,
        };

        assert!(album_config.is_album_view);
        assert!(!artist_config.is_album_view);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_search_empty_state_default() {
        let empty_state = SearchEmptyState::default();
        assert!(empty_state.config.is_album_view);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_search_empty_state_builder() {
        let empty_state = SearchEmptyState::builder().is_album_view(false).build();

        assert!(!empty_state.config.is_album_view);
    }
}

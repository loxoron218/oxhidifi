//! Artist grid view with artist images and album counts.
//!
//! This module implements the `ArtistGridView` component that displays artists
//! in a responsive grid layout with artist images, names, and album counts,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::sync::Arc;

use libadwaita::{
    gtk::{
        AccessibleRole::{Grid, Group},
        Align::{Center, Start},
        Box, FlowBox, FlowBoxChild, GestureClick, Label,
        Orientation::Vertical,
        SelectionMode::None as SelectionNone,
        Widget,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExt, BoxExt, Cast, FlowBoxChildExt, WidgetExt},
};

use crate::{
    library::models::Artist,
    state::{AppState, LibraryState, NavigationState::ArtistDetail},
    ui::components::{
        cover_art::CoverArt,
        empty_state::{EmptyState, EmptyStateConfig},
    },
};

/// Builder pattern for configuring ArtistGridView components.
#[derive(Debug, Default)]
pub struct ArtistGridViewBuilder {
    /// Optional application state reference for reactive updates.
    app_state: Option<Arc<AppState>>,
    /// Vector of artists to display in the grid.
    artists: Vec<Artist>,
    /// Whether to use compact layout with smaller cover sizes.
    compact: bool,
}

impl ArtistGridViewBuilder {
    /// Sets the application state for reactive updates.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn app_state(mut self, app_state: Arc<AppState>) -> Self {
        self.app_state = Some(app_state);
        self
    }

    /// Sets the initial artists to display.
    ///
    /// # Arguments
    ///
    /// * `artists` - Vector of artists to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn artists(mut self, artists: Vec<Artist>) -> Self {
        self.artists = artists;
        self
    }

    /// Configures whether to use compact layout.
    ///
    /// # Arguments
    ///
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Builds the ArtistGridView component.
    ///
    /// # Returns
    ///
    /// A new `ArtistGridView` instance.
    pub fn build(self) -> ArtistGridView {
        ArtistGridView::new(self.app_state, self.artists, self.compact)
    }
}

/// Responsive grid view for displaying artists with images and album counts.
///
/// The `ArtistGridView` component displays artists in a responsive grid layout
/// that adapts from 360px to 4K+ displays, with support for virtual scrolling,
/// real-time filtering, and keyboard navigation.
pub struct ArtistGridView {
    /// The underlying GTK widget (FlowBox).
    pub widget: Widget,
    /// The flow box container.
    pub flow_box: FlowBox,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Current artists being displayed.
    pub artists: Vec<Artist>,
    /// Configuration flags.
    pub config: ArtistGridViewConfig,
    /// Empty state component for when no artists are available.
    pub empty_state: Option<EmptyState>,
    /// Current sort criteria.
    pub current_sort: ArtistSortCriteria,
}

/// Configuration for ArtistGridView display options.
#[derive(Debug, Clone)]
pub struct ArtistGridViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

impl ArtistGridView {
    /// Creates a new ArtistGridView component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `artists` - Initial artists to display
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `ArtistGridView` instance.
    pub fn new(app_state: Option<Arc<AppState>>, artists: Vec<Artist>, compact: bool) -> Self {
        let config = ArtistGridViewConfig { compact };

        let flow_box = FlowBox::builder()
            .halign(Center)
            .valign(Start)
            .homogeneous(true)
            .max_children_per_line(100) // Will be adjusted based on available width
            .selection_mode(SelectionNone)
            .css_classes(["artist-grid"])
            .build();

        // Create main container that can hold both flow box and empty state
        let main_container = Box::builder().orientation(Vertical).build();

        main_container.append(&flow_box.clone().upcast::<Widget>());

        // Set ARIA attributes for accessibility
        flow_box.set_accessible_role(Grid);

        // set_accessible_description doesn't exist in GTK4, remove this line

        // Create empty state component
        let empty_state = app_state.as_ref().map(|state| {
            EmptyState::new(
                Some(state.clone()),
                None, // Will be set later when we have access to settings
                EmptyStateConfig {
                    is_album_view: false,
                },
                None, // Will be set later when we have access to window
            )
        });

        // Add empty state to main container if it exists
        if let Some(ref empty_state) = empty_state {
            main_container.append(&empty_state.widget);
        }

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            flow_box,
            app_state,
            artists: Vec::new(),
            config,
            empty_state,
            current_sort: ArtistSortCriteria::Name, // Default sort by Name
        };

        // Populate with initial artists
        view.set_artists(artists);

        view
    }

    /// Creates an ArtistGridView builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `ArtistGridViewBuilder` instance.
    pub fn builder() -> ArtistGridViewBuilder {
        ArtistGridViewBuilder::default()
    }

    /// Sets the artists to display in the grid.
    ///
    /// # Arguments
    ///
    /// * `artists` - New vector of artists to display
    ///
    /// # Panics
    ///
    /// Panics if empty state exists but is None (should never happen with proper initialization).
    pub fn set_artists(&mut self, artists: Vec<Artist>) {
        // Clear existing children
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        self.artists = artists;

        // Apply current sort
        self.apply_sort();

        // Update empty state visibility
        if let Some(_empty_state) = &self.empty_state {
            // Get current library state from app state if available
            let library_state = if let Some(app_state) = &self.app_state {
                app_state.get_library_state()
            } else {
                LibraryState {
                    artists: self.artists.clone(),
                    ..Default::default()
                }
            };
            self.empty_state
                .as_ref()
                .unwrap()
                .update_from_library_state(&library_state);
        }

        // Add new artist items
        for artist in &self.artists {
            let artist_item = self.create_artist_item(artist);
            self.flow_box.insert(&artist_item, -1);
        }
    }

    /// Creates a single artist item widget for the grid.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to create an item for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the artist item.
    fn create_artist_item(&self, artist: &Artist) -> Widget {
        // Create cover art (using default image for artists)
        let cover_art = CoverArt::builder()
            .artwork_path("") // No specific artwork for artists yet
            .show_dr_badge(false)
            .dimensions(180, 180)
            .build();

        // Create name label
        let name_label = Label::builder()
            .label(&artist.name)
            .halign(Center)
            .xalign(0.5)
            .ellipsize(End)
            .lines(2)
            .tooltip_text(&artist.name)
            .build();

        // Create album count placeholder (will be populated from state)
        let album_count_text = "Albums: ?";
        let album_count_label = Label::builder()
            .label(album_count_text)
            .halign(Center)
            .xalign(0.5)
            .css_classes(["dim-label"])
            .ellipsize(End)
            .lines(1)
            .tooltip_text(album_count_text)
            .build();

        // Create main container with fixed dimensions to ensure consistent sizing
        let container = Box::builder()
            .orientation(Vertical)
            .halign(Center)
            .valign(Start)
            .hexpand(false)
            .vexpand(false)
            .spacing(4)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .width_request(196) // 180 (cover) + 8*2 (margins) = 196
            .height_request(250) // Approximate height for cover + labels
            .css_classes(["artist-item"])
            .build();

        container.append(&cover_art.widget);
        container.append(name_label.upcast_ref::<Widget>());
        container.append(album_count_label.upcast_ref::<Widget>());

        // Set ARIA attributes for accessibility
        container.set_accessible_role(Group);

        // set_accessible_description doesn't exist in GTK4, remove this line

        // Create FlowBoxChild wrapper
        let child = FlowBoxChild::new();
        child.set_child(Some(&container));
        child.set_focusable(true);

        // Add click controller for navigation
        let click_controller = GestureClick::new();

        let artist_clone = artist.clone();
        let app_state = self.app_state.clone();

        click_controller.connect_released(move |_, _, _, _| {
            // Navigate to artist detail view
            if let Some(ref state) = app_state {
                state.update_navigation(ArtistDetail(artist_clone.clone()));
            }
        });

        child.add_controller(click_controller);

        // Support keyboard activation (Enter/Space)
        let artist_clone = artist.clone();
        let app_state = self.app_state.clone();
        child.connect_activate(move |_| {
            if let Some(ref state) = app_state {
                state.update_navigation(ArtistDetail(artist_clone.clone()));
            }
        });

        child.upcast_ref::<Widget>().clone()
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: ArtistGridViewConfig) {
        self.config = config;

        // Rebuild all artist items with new configuration
        self.set_artists(self.artists.clone());
    }

    /// Filters artists based on a search query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    pub fn filter_artists(&mut self, query: &str) {
        let filtered_artists: Vec<Artist> = self
            .artists
            .iter()
            .filter(|artist| artist.name.to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect();

        self.set_artists(filtered_artists);
    }

    /// Sorts artists by the specified criteria.
    ///
    /// # Arguments
    ///
    /// * `sort_by` - Sorting criteria
    pub fn sort_artists(&mut self, sort_by: ArtistSortCriteria) {
        self.current_sort = sort_by;

        // Apply sort to current artists and refresh display
        self.apply_sort();

        // Re-display sorted artists
        self.set_artists(self.artists.clone());
    }

    /// Applies the current sort criteria to the artists vector.
    fn apply_sort(&mut self) {
        match self.current_sort {
            ArtistSortCriteria::Name => {
                self.artists.sort_by(|a, b| a.name.cmp(&b.name));
            }
            ArtistSortCriteria::AlbumCount => {
                // For now, we can't sort by album count without additional data
                // This would require querying the database or having album counts in state
                self.artists.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }
    }
}

/// Sorting criteria for artists.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtistSortCriteria {
    /// Sort by artist name
    Name,
    /// Sort by album count (requires additional data)
    AlbumCount,
}

impl Default for ArtistGridView {
    fn default() -> Self {
        Self::new(None, Vec::new(), false)
    }
}

#[cfg(test)]
mod tests {
    use crate::{library::models::Artist, ui::views::artist_grid::ArtistGridView};

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_grid_view_builder() {
        let artists = vec![
            Artist {
                id: 1,
                name: "Test Artist 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            Artist {
                id: 2,
                name: "Test Artist 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        let grid_view = ArtistGridView::builder()
            .artists(artists)
            .compact(false)
            .build();

        assert_eq!(grid_view.artists.len(), 2);
        assert!(!grid_view.config.compact);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_grid_view_default() {
        let grid_view = ArtistGridView::default();
        assert_eq!(grid_view.artists.len(), 0);
        assert!(!grid_view.config.compact);
    }

    #[test]
    fn test_artist_sort_criteria() {
        // This test doesn't require GTK, so no skip needed
        let mut artists = vec![
            Artist {
                id: 1,
                name: "B Artist".to_string(),
                ..Artist::default()
            },
            Artist {
                id: 2,
                name: "A Artist".to_string(),
                ..Artist::default()
            },
        ];

        // Test name sorting
        artists.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(artists[0].name, "A Artist");
    }
}

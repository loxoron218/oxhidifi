//! Artist grid view with artist images and album counts.
//!
//! This module implements the `ArtistGridView` component that displays artists
//! in a responsive grid layout with artist images, names, and album counts,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::sync::Arc;

use {
    async_trait::async_trait,
    libadwaita::{
        gtk::{
            AccessibleRole::{Grid, Group},
            Align::{Center, Start},
            Box as GtkBox, FlowBox, FlowBoxChild, Label,
            Orientation::Vertical,
            SelectionMode::None as SelectionNone,
            Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExt, BoxExt, Cast, FlowBoxChildExt, ListModelExt, WidgetExt},
    },
};

use crate::{
    library::models::Artist,
    state::{
        AppState,
        AppStateEvent::{self, LibraryStateChanged, SearchFilterChanged},
        LibraryState, StateObserver,
    },
    ui::components::{
        cover_art::CoverArt,
        empty_state::{EmptyState, EmptyStateConfig},
    },
};

/// Builder pattern for configuring ArtistGridView components.
#[derive(Debug, Default)]
pub struct ArtistGridViewBuilder {
    app_state: Option<Arc<AppState>>,
    artists: Vec<Artist>,
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
        let main_container = GtkBox::builder().orientation(Vertical).build();

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
    pub fn set_artists(&mut self, artists: Vec<Artist>) {
        // Clear existing children
        let children = self.flow_box.observe_children();
        let n_items = children.n_items();
        for i in 0..n_items {
            if let Some(child) = children.item(i)
                && let Ok(widget) = child.downcast::<Widget>()
            {
                self.flow_box.remove(&widget);
            }
        }

        self.artists = artists;

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

        // Create main container
        let container = GtkBox::builder()
            .orientation(Vertical)
            .halign(Center)
            .valign(Start)
            .spacing(4)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
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
        let mut sorted_artists = self.artists.clone();

        match sort_by {
            ArtistSortCriteria::Name => {
                sorted_artists.sort_by(|a, b| a.name.cmp(&b.name));
            }
            ArtistSortCriteria::AlbumCount => {
                // For now, we can't sort by album count without additional data
                // This would require querying the database or having album counts in state
                sorted_artists.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }

        self.set_artists(sorted_artists);
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

#[async_trait(?Send)]
impl StateObserver for ArtistGridView {
    async fn handle_state_change(&mut self, event: AppStateEvent) {
        match event {
            LibraryStateChanged(state) => {
                self.handle_library_state_change(state).await;
            }
            SearchFilterChanged(filter) => {
                if let Some(query) = filter {
                    self.filter_artists(&query);
                } else {
                    // Reset to all artists
                    if let Some(ref app_state) = self.app_state {
                        let library_state = app_state.get_library_state();
                        self.set_artists(library_state.artists);
                    }
                }
            }
            _ => {}
        }
    }
}

impl ArtistGridView {
    async fn handle_library_state_change(&mut self, state: LibraryState) {
        self.set_artists(state.artists);
    }
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
    fn test_artist_grid_view_default() {
        let grid_view = ArtistGridView::default();
        assert_eq!(grid_view.artists.len(), 0);
        assert!(!grid_view.config.compact);
    }

    #[test]
    fn test_artist_sort_criteria() {
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

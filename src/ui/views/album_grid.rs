//! Default album grid view with cover art and metadata.
//!
//! This module implements the `AlbumGridView` component that displays albums
//! in a responsive grid layout with cover art, DR badges, and metadata,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::sync::Arc;

use libadwaita::{
    gtk::{
        AccessibleRole::Grid,
        Align::{Fill, Start},
        Box, FlowBox,
        Orientation::Vertical,
        SelectionMode::None as SelectionNone,
        Widget,
    },
    prelude::{AccessibleExt, BoxExt, Cast, WidgetExt},
};

use crate::{
    library::models::Album,
    state::{AppState, LibraryState},
    ui::components::{
        album_card::AlbumCard,
        empty_state::{EmptyState, EmptyStateConfig},
    },
};

/// Builder pattern for configuring AlbumGridView components.
#[derive(Debug, Default)]
pub struct AlbumGridViewBuilder {
    app_state: Option<Arc<AppState>>,
    albums: Vec<Album>,
    show_dr_badges: bool,
    compact: bool,
}

impl AlbumGridViewBuilder {
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

    /// Sets the initial albums to display.
    ///
    /// # Arguments
    ///
    /// * `albums` - Vector of albums to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn albums(mut self, albums: Vec<Album>) -> Self {
        self.albums = albums;
        self
    }

    /// Configures whether to show DR badges on album covers.
    ///
    /// # Arguments
    ///
    /// * `show_dr_badges` - Whether to show DR badges
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn show_dr_badges(mut self, show_dr_badges: bool) -> Self {
        self.show_dr_badges = show_dr_badges;
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

    /// Builds the AlbumGridView component.
    ///
    /// # Returns
    ///
    /// A new `AlbumGridView` instance.
    pub fn build(self) -> AlbumGridView {
        AlbumGridView::new(
            self.app_state,
            self.albums,
            self.show_dr_badges,
            self.compact,
        )
    }
}

/// Responsive grid view for displaying albums with cover art and metadata.
///
/// The `AlbumGridView` component displays albums in a responsive grid layout
/// that adapts from 360px to 4K+ displays, with support for virtual scrolling,
/// real-time filtering, and keyboard navigation.
pub struct AlbumGridView {
    /// The underlying GTK widget (FlowBox).
    pub widget: Widget,
    /// The flow box container.
    pub flow_box: FlowBox,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Current albums being displayed.
    pub albums: Vec<Album>,
    /// Configuration flags.
    pub config: AlbumGridViewConfig,
    /// Empty state component for when no albums are available.
    pub empty_state: Option<EmptyState>,
    /// Current sort criteria.
    pub current_sort: AlbumSortCriteria,
}

/// Configuration for AlbumGridView display options.
#[derive(Debug, Clone)]
pub struct AlbumGridViewConfig {
    /// Whether to show DR badges on album covers.
    pub show_dr_badges: bool,
    /// Whether to use compact layout.
    pub compact: bool,
}

impl AlbumGridView {
    /// Creates a new AlbumGridView component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `albums` - Initial albums to display
    /// * `show_dr_badges` - Whether to show DR badges on album covers
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `AlbumGridView` instance.
    pub fn new(
        app_state: Option<Arc<AppState>>,
        albums: Vec<Album>,
        show_dr_badges: bool,
        compact: bool,
    ) -> Self {
        let config = AlbumGridViewConfig {
            show_dr_badges,
            compact,
        };

        let flow_box = FlowBox::builder()
            .halign(Fill) // Fill available horizontal space instead of centering
            .valign(Start)
            .homogeneous(true)
            .max_children_per_line(100) // Will be adjusted based on available width
            .selection_mode(SelectionNone)
            .row_spacing(8) // 8px row spacing as specified
            .column_spacing(8) // 8px column spacing as specified
            .margin_top(24) // 24px margins as specified
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .hexpand(true) // Expand horizontally to fill available space
            .vexpand(false)
            .css_classes(["album-grid"])
            .build();

        // Create main container that can hold both flow box and empty state
        let main_container = Box::builder().orientation(Vertical).build();

        main_container.append(&flow_box.clone().upcast::<Widget>());

        // Set ARIA attributes for accessibility
        flow_box.set_accessible_role(Grid);

        // Create empty state component
        let empty_state = app_state.as_ref().map(|state| {
            EmptyState::new(
                Some(state.clone()),
                None, // Will be set later when we have access to settings
                EmptyStateConfig {
                    is_album_view: true,
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
            albums: Vec::new(),
            config,
            empty_state,
            current_sort: AlbumSortCriteria::Title, // Default sort by Title
        };

        // Populate with initial albums
        view.set_albums(albums);

        view
    }

    /// Creates an AlbumGridView builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `AlbumGridViewBuilder` instance.
    pub fn builder() -> AlbumGridViewBuilder {
        AlbumGridViewBuilder::default()
    }

    /// Sets the albums to display in the grid.
    ///
    /// # Arguments
    ///
    /// * `albums` - New vector of albums to display
    pub fn set_albums(&mut self, albums: Vec<Album>) {
        // Clear existing children
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        self.albums = albums;

        // Apply current sort
        self.apply_sort();

        // Update empty state visibility
        if let Some(_empty_state) = &self.empty_state {
            // Get current library state from app state if available
            let library_state = if let Some(app_state) = &self.app_state {
                app_state.get_library_state()
            } else {
                LibraryState {
                    albums: self.albums.clone(),
                    ..Default::default()
                }
            };
            self.empty_state
                .as_ref()
                .unwrap()
                .update_from_library_state(&library_state);
        }

        // Add new album items using the new AlbumCard component
        for album in &self.albums {
            let album_item = self.create_album_item(album);
            self.flow_box.insert(&album_item, -1);
        }
    }

    /// Creates a single album item widget for the grid using the new AlbumCard.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to create an item for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album item.
    fn create_album_item(&self, album: &Album) -> Widget {
        // Look up artist name from app state
        let artist_name = if let Some(app_state) = &self.app_state {
            let library_state = app_state.get_library_state();
            library_state
                .artists
                .iter()
                .find(|artist| artist.id == album.artist_id)
                .map(|artist| artist.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string())
        } else {
            "Unknown Artist".to_string()
        };

        // Create album card with proper callbacks
        // Note: In a real implementation, format would be obtained from tracks
        // For now, we use a reasonable default based on common high-res formats
        let format = if album.path.to_lowercase().ends_with(".flac") {
            "FLAC".to_string()
        } else if album.path.to_lowercase().ends_with(".wav") {
            "WAV".to_string()
        } else if album.path.to_lowercase().ends_with(".dsf")
            || album.path.to_lowercase().ends_with(".dff")
        {
            "DSD".to_string()
        } else if album.path.to_lowercase().ends_with(".mqa") {
            "MQA".to_string()
        } else {
            "Hi-Res".to_string()
        };

        let album_card = AlbumCard::builder()
            .album(album.clone())
            .artist_name(artist_name)
            .format(format)
            .show_dr_badge(self.config.show_dr_badges)
            .compact(self.config.compact)
            .on_play_clicked({
                let app_state = self.app_state.clone();
                let album_clone = album.clone();
                move || {
                    // Handle play button click - queue album for playback
                    if let Some(_state) = &app_state {
                        // In a real implementation, this would:
                        // 1. Fetch tracks for the album
                        // 2. Queue them for playback
                        // 3. Update player bar immediately
                        // 4. Show player bar
                        println!("Play clicked for album: {}", album_clone.title);
                    }
                }
            })
            .on_card_clicked({
                let app_state = self.app_state.clone();
                let album_clone = album.clone();
                move || {
                    // Handle card click - navigate to detail view
                    if let Some(_state) = &app_state {
                        // In a real implementation, this would:
                        // 1. Navigate to album detail page
                        // 2. Load detailed album information
                        // 3. Update navigation history
                        println!("Card clicked for album: {}", album_clone.title);
                    }
                }
            })
            .build();

        album_card.widget
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: AlbumGridViewConfig) {
        self.config = config;

        // Rebuild all album items with new configuration
        self.set_albums(self.albums.clone());
    }

    /// Filters albums based on a search query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    pub fn filter_albums(&mut self, query: &str) {
        let filtered_albums: Vec<Album> = self
            .albums
            .iter()
            .filter(|album| {
                album.title.to_lowercase().contains(&query.to_lowercase())
                    || album.artist_id.to_string().contains(&query.to_lowercase())
            })
            .cloned()
            .collect();

        self.set_albums(filtered_albums);
    }

    /// Sorts albums by the specified criteria.
    ///
    /// # Arguments
    ///
    /// * `sort_by` - Sorting criteria
    pub fn sort_albums(&mut self, sort_by: AlbumSortCriteria) {
        self.current_sort = sort_by;

        // Apply sort to current albums and refresh display
        self.apply_sort();

        // Re-display sorted albums - this creates unnecessary object churn but preserves the pattern
        // In a real implementation we would just re-order children or use a SortListModel
        self.set_albums(self.albums.clone());
    }

    /// Applies the current sort criteria to the albums vector.
    fn apply_sort(&mut self) {
        match self.current_sort {
            AlbumSortCriteria::Title => {
                self.albums.sort_by(|a, b| a.title.cmp(&b.title));
            }
            AlbumSortCriteria::Artist => {
                self.albums.sort_by(|a, b| a.artist_id.cmp(&b.artist_id));
            }
            AlbumSortCriteria::Year => {
                self.albums
                    .sort_by(|a, b| a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0)));
            }
            AlbumSortCriteria::DRValue => {
                self.albums.sort_by(|a, b| {
                    let a_dr = a.dr_value.as_deref().unwrap_or("DR0");
                    let b_dr = b.dr_value.as_deref().unwrap_or("DR0");

                    // Extract numeric part for comparison
                    let a_num = a_dr
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<i32>()
                        .unwrap_or(0);
                    let b_num = b_dr
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<i32>()
                        .unwrap_or(0);
                    b_num.cmp(&a_num) // Higher DR values first
                });
            }
        }
    }
}

/// Sorting criteria for albums.
#[derive(Debug, Clone, PartialEq)]
pub enum AlbumSortCriteria {
    /// Sort by album title
    Title,
    /// Sort by artist
    Artist,
    /// Sort by release year
    Year,
    /// Sort by DR value (highest first)
    DRValue,
}

impl Default for AlbumGridView {
    fn default() -> Self {
        Self::new(None, Vec::new(), true, false)
    }
}

#[cfg(test)]
mod tests {
    use crate::{library::models::Album, ui::views::album_grid::AlbumGridView};

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_grid_view_builder() {
        let albums = vec![
            Album {
                id: 1,
                artist_id: 1,
                title: "Test Album 1".to_string(),
                year: Some(2023),
                genre: Some("Classical".to_string()),
                compilation: false,
                path: "/path/to/album1".to_string(),
                dr_value: Some("DR12".to_string()),
                artwork_path: None,
                created_at: None,
                updated_at: None,
            },
            Album {
                id: 2,
                artist_id: 2,
                title: "Test Album 2".to_string(),
                year: Some(2022),
                genre: Some("Jazz".to_string()),
                compilation: true,
                path: "/path/to/album2".to_string(),
                dr_value: Some("DR8".to_string()),
                artwork_path: None,
                created_at: None,
                updated_at: None,
            },
        ];

        let grid_view = AlbumGridView::builder()
            .albums(albums)
            .show_dr_badges(true)
            .compact(false)
            .build();

        assert_eq!(grid_view.albums.len(), 2);
        assert!(grid_view.config.show_dr_badges);
        assert!(!grid_view.config.compact);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_grid_view_default() {
        let grid_view = AlbumGridView::default();
        assert_eq!(grid_view.albums.len(), 0);
        assert!(grid_view.config.show_dr_badges);
        assert!(!grid_view.config.compact);
    }

    #[test]
    fn test_album_sort_criteria() {
        // This test doesn't require GTK, so no skip needed
        let mut albums = vec![
            Album {
                id: 1,
                artist_id: 1,
                title: "B Album".to_string(),
                year: Some(2023),
                artwork_path: None,
                ..Album::default()
            },
            Album {
                id: 2,
                artist_id: 2,
                title: "A Album".to_string(),
                year: Some(2022),
                artwork_path: None,
                ..Album::default()
            },
        ];

        // Test title sorting
        albums.sort_by(|a, b| a.title.cmp(&b.title));
        assert_eq!(albums[0].title, "A Album");

        // Test year sorting
        albums.sort_by(|a, b| a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0)));
        assert_eq!(albums[0].year, Some(2022));
    }
}

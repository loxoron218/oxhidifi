//! Default album grid view with cover art and metadata.
//!
//! This module implements the `AlbumGridView` component that displays albums
//! in a responsive grid layout with cover art, DR badges, and metadata,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::sync::Arc;

use libadwaita::{
    gtk::{
        Align::{Center, Start},
        Box as GtkBox,
        FlowBox,
        FlowBoxChild,
        Label,
        Orientation::Vertical,
        Widget,
    },
    prelude::{BoxExt, FlowBoxExt, LabelExt, WidgetExt},
};

use crate::{
    library::models::Album,
    state::{AppState, LibraryState, StateObserver},
    ui::components::{cover_art::CoverArt, hifi_metadata::HiFiMetadata},
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
            .halign(Center)
            .valign(Start)
            .homogeneous(true)
            .max_children_per_line(100) // Will be adjusted based on available width
            .selection_mode(libadwaita::gtk::SelectionMode::None)
            .css_classes(vec!["album-grid".to_string()])
            .build();

        // Set ARIA attributes for accessibility
        flow_box.set_accessible_role(libadwaita::gtk::AccessibleRole::Grid);
        flow_box.set_accessible_description(Some("Album grid view"));

        let mut view = Self {
            widget: flow_box.clone().upcast::<Widget>(),
            flow_box,
            app_state,
            albums: Vec::new(),
            config,
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
        self.flow_box.foreach(|child| {
            self.flow_box.remove(child);
        });

        self.albums = albums;

        // Add new album items
        for album in &self.albums {
            let album_item = self.create_album_item(album);
            self.flow_box.insert(&album_item, -1);
        }
    }

    /// Creates a single album item widget for the grid.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to create an item for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album item.
    fn create_album_item(&self, album: &Album) -> Widget {
        // Create cover art
        let cover_art = CoverArt::builder()
            .artwork_path(&album.path)
            .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
            .show_dr_badge(self.config.show_dr_badges)
            .dimensions(180, 180)
            .build();

        // Create title label
        let title_label = Label::builder()
            .label(&album.title)
            .halign(Center)
            .xalign(0.5)
            .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
            .lines(2)
            .tooltip_text(&album.title)
            .build();

        // Create artist/year info
        let artist_year_text = if let Some(year) = album.year {
            format!("{} ({})", album.artist_id, year)
        } else {
            album.artist_id.to_string()
        };

        let artist_year_label = Label::builder()
            .label(&artist_year_text)
            .halign(Center)
            .xalign(0.5)
            .css_classes(vec!["dim-label".to_string()])
            .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
            .lines(1)
            .tooltip_text(&artist_year_text)
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
            .css_classes(vec!["album-item".to_string()])
            .build();

        container.append(&cover_art.widget);
        container.append(&title_label.upcast::<Widget>());
        container.append(&artist_year_label.upcast::<Widget>());

        // Set ARIA attributes for accessibility
        container.set_accessible_role(libadwaita::gtk::AccessibleRole::Group);
        container.set_accessible_description(Some(&format!(
            "Album: {}, Artist ID: {}",
            album.title, album.artist_id
        )));

        // Create FlowBoxChild wrapper
        let child = FlowBoxChild::new();
        child.set_child(Some(&container));
        child.set_focusable(true);

        child.upcast::<Widget>()
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
        let mut sorted_albums = self.albums.clone();
        
        match sort_by {
            AlbumSortCriteria::Title => {
                sorted_albums.sort_by(|a, b| a.title.cmp(&b.title));
            }
            AlbumSortCriteria::Artist => {
                sorted_albums.sort_by(|a, b| a.artist_id.cmp(&b.artist_id));
            }
            AlbumSortCriteria::Year => {
                sorted_albums.sort_by(|a, b| {
                    a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0))
                });
            }
            AlbumSortCriteria::DRValue => {
                sorted_albums.sort_by(|a, b| {
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

        self.set_albums(sorted_albums);
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

#[async_trait::async_trait]
impl StateObserver for AlbumGridView {
    async fn handle_state_change(&mut self, event: crate::state::AppStateEvent) {
        match event {
            crate::state::AppStateEvent::LibraryStateChanged(state) => {
                self.handle_library_state_change(state).await;
            }
            crate::state::AppStateEvent::SearchFilterChanged(filter) => {
                if let Some(query) = filter {
                    self.filter_albums(&query);
                } else {
                    // Reset to all albums
                    if let Some(ref app_state) = self.app_state {
                        let library_state = app_state.get_library_state();
                        self.set_albums(library_state.albums);
                    }
                }
            }
            _ => {}
        }
    }
}

impl AlbumGridView {
    async fn handle_library_state_change(&mut self, state: LibraryState) {
        self.set_albums(state.albums);
    }
}

impl Default for AlbumGridView {
    fn default() -> Self {
        Self::new(None, Vec::new(), true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::models::Album;

    #[test]
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
    fn test_album_grid_view_default() {
        let grid_view = AlbumGridView::default();
        assert_eq!(grid_view.albums.len(), 0);
        assert!(grid_view.config.show_dr_badges);
        assert!(!grid_view.config.compact);
    }

    #[test]
    fn test_album_sort_criteria() {
        let mut albums = vec![
            Album {
                id: 1,
                artist_id: 1,
                title: "B Album".to_string(),
                year: Some(2023),
                ..Album::default()
            },
            Album {
                id: 2,
                artist_id: 2,
                title: "A Album".to_string(),
                year: Some(2022),
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
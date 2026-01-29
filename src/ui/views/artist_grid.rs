//! Artist grid view with artist images and album counts.
//!
//! This module implements the `ArtistGridView` component that displays artists
//! in a responsive grid layout with artist images, names, and album counts,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::{cell::RefCell, convert::TryFrom, rc::Rc, sync::Arc};

use {
    libadwaita::{
        glib::{JoinHandle, MainContext},
        gtk::{
            AccessibleRole::{Grid, Group},
            Align::{Fill, Start},
            Box, FlowBox, FlowBoxChild, GestureClick, Label,
            Orientation::Vertical,
            SelectionMode::None as SelectionNone,
            Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExt, BoxExt, Cast, FlowBoxChildExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    error::domain::UiError::{self, BuilderError},
    library::models::Artist,
    state::{AppState, LibraryState, NavigationState::ArtistDetail, ZoomEvent::GridZoomChanged},
    ui::components::{
        cover_art::CoverArt,
        empty_state::{EmptyState, EmptyStateConfig},
    },
};

/// Builder pattern for configuring `ArtistGridView` components.
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Builds the `ArtistGridView` component.
    ///
    /// # Returns
    ///
    /// A new `ArtistGridView` instance.
    #[must_use]
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
    /// The underlying GTK widget (`FlowBox`).
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
    /// Shared reference to artist cards for zoom updates.
    artist_cards_ref: Rc<RefCell<Vec<Rc<ArtistCard>>>>,
    /// Zoom subscription handle for cleanup.
    zoom_subscription_handle: Option<JoinHandle<()>>,
}

/// Configuration for `ArtistGridView` display options.
#[derive(Debug, Clone)]
pub struct ArtistGridViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

impl ArtistGridView {
    /// Creates a new `ArtistGridView` component.
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
    #[must_use]
    pub fn new(app_state: Option<Arc<AppState>>, artists: Vec<Artist>, compact: bool) -> Self {
        let config = ArtistGridViewConfig { compact };

        let flow_box = FlowBox::builder()
            .halign(Fill)
            .valign(Start)
            .homogeneous(true)
            .max_children_per_line(100)
            .selection_mode(SelectionNone)
            .row_spacing(8)
            .column_spacing(8)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .hexpand(true)
            .vexpand(false)
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

        let artist_cards_ref = Rc::new(RefCell::new(Vec::new()));

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            flow_box: flow_box.clone(),
            app_state: app_state.clone(),
            artists: Vec::new(),
            config,
            empty_state,
            current_sort: ArtistSortCriteria::Name,
            artist_cards_ref: artist_cards_ref.clone(),
            zoom_subscription_handle: if let Some(state) = app_state {
                let state_clone = state.clone();
                let flow_box_clone = flow_box.clone();
                let artist_cards_ref_clone = artist_cards_ref.clone();
                let handle = MainContext::default().spawn_local(async move {
                    let rx = state_clone.zoom_manager.subscribe();
                    while let Ok(event) = rx.recv().await {
                        if let GridZoomChanged(_) = event {
                            let cover_size = state_clone.zoom_manager.get_grid_cover_dimensions().0;
                            let cover_size_u32 = u32::try_from(cover_size).unwrap_or_else(|_| {
                                error!("Invalid cover size {cover_size}, using default 180");
                                180
                            });

                            let cards = artist_cards_ref_clone.borrow();
                            for card in cards.iter() {
                                if let Err(e) = card.update_cover_size(cover_size_u32) {
                                    error!("Failed to update cover size: {e}");
                                }
                            }

                            flow_box_clone.queue_draw();
                        }
                    }
                });
                Some(handle)
            } else {
                None
            },
        };

        view.set_artists(artists);

        view
    }

    /// Creates an `ArtistGridView` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `ArtistGridViewBuilder` instance.
    #[must_use]
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
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        // Clear existing artist cards
        self.artist_cards_ref.borrow_mut().clear();

        self.artists = artists;

        // Apply current sort
        self.apply_sort();

        // Update empty state visibility
        if let Some(ref empty_state) = self.empty_state {
            // Get current library state from app state if available
            let library_state = if let Some(app_state) = &self.app_state {
                app_state.get_library_state()
            } else {
                LibraryState {
                    artists: self.artists.clone(),
                    ..Default::default()
                }
            };
            empty_state.update_from_library_state(&library_state);
        }

        let cover_size = self.get_cover_size();

        // Add new artist items
        for artist in &self.artists {
            let artist_card = match self.create_artist_card(artist, cover_size) {
                Ok(card) => card,
                Err(e) => {
                    error!("Failed to create artist card for '{}': {e}", artist.name);
                    continue;
                }
            };
            let card_arc = Rc::new(artist_card);
            self.flow_box.insert(&card_arc.widget, -1);
            self.artist_cards_ref.borrow_mut().push(card_arc);
        }
    }

    /// Gets the cover size for artist cards based on current configuration.
    ///
    /// # Returns
    ///
    /// The cover size in pixels.
    #[must_use]
    fn get_cover_size(&self) -> i32 {
        if let Some(app_state) = &self.app_state {
            app_state.zoom_manager.get_grid_cover_dimensions().0
        } else if self.config.compact {
            120
        } else {
            180
        }
    }

    /// Creates a single artist card for the grid.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to create a card for
    /// * `cover_size` - The size of cover art in pixels
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `ArtistCard` instance or a `UiError` if
    /// card creation fails.
    fn create_artist_card(&self, artist: &Artist, cover_size: i32) -> Result<ArtistCard, UiError> {
        let artist_clone = artist.clone();
        let app_state = self.app_state.clone();

        let cover_size_u32 = u32::try_from(cover_size).unwrap_or_else(|_| {
            error!("Invalid cover size {cover_size}, using default 180");
            180
        });
        ArtistCard::builder()
            .artist(artist.clone())
            .cover_size(cover_size_u32)
            .on_card_clicked(move || {
                if let Some(ref state) = app_state {
                    state.update_navigation(ArtistDetail(artist_clone.clone()));
                }
            })
            .build()
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

    /// Stops the zoom subscription and cleans up resources.
    pub fn cleanup(&mut self) {
        if let Some(handle) = self.zoom_subscription_handle.take() {
            handle.abort();
        }
    }
}

impl Drop for ArtistGridView {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Artist card component with cover art and metadata.
///
/// The `ArtistCard` component displays artists with cover art, names, and
/// album counts, matching the album card styling.
#[derive(Clone)]
pub struct ArtistCard {
    /// The underlying `FlowBoxChild` widget.
    pub widget: Widget,
    /// The main artist tile container.
    pub artist_tile: Box,
    /// The cover art component.
    pub cover_art: CoverArt,
    /// Artist name label.
    pub name_label: Label,
    /// Album count label.
    pub album_count_label: Label,
}

impl ArtistCard {
    /// Creates a new `ArtistCard` component.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to display
    /// * `cover_size` - The size of the cover art in pixels
    /// * `on_card_clicked` - Optional callback for card clicks
    ///
    /// # Returns
    ///
    /// A new `ArtistCard` instance.
    ///
    /// # Panics
    ///
    /// Panics if `cover_size` or the calculated `max_width_chars` values
    /// cannot be converted to i32. This indicates a programming error as
    /// the calculation logic should always produce valid values.
    #[must_use]
    pub fn new(artist: &Artist, cover_size: u32, on_card_clicked: Option<Rc<dyn Fn()>>) -> Self {
        let cover_size_i32 = i32::try_from(cover_size).expect("Cover size (u32) should fit in i32");
        let (cover_width, cover_height) = (cover_size_i32, cover_size_i32);

        let cover_art = CoverArt::builder()
            .icon_name("avatar-default-symbolic")
            .show_dr_badge(false)
            .dimensions(cover_width, cover_height)
            .build();

        let name_max_width = ((cover_size - 16) / 10).max(8);
        let name_max_width_i32 = i32::try_from(name_max_width)
            .expect("max_width_chars calculation should always result in valid i32");
        let name_label = Label::builder()
            .label(&artist.name)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(2)
            .max_width_chars(name_max_width_i32)
            .tooltip_text(&artist.name)
            .css_classes(["album-title-label"])
            .build();

        let album_count_text = "Albums";
        let album_count_max_width = ((cover_size - 16) / 10).max(8);
        let album_count_max_width_i32 = i32::try_from(album_count_max_width)
            .expect("album_count max_width_chars calculation should always result in valid i32");
        let album_count_label = Label::builder()
            .label(album_count_text)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(1)
            .max_width_chars(album_count_max_width_i32)
            .tooltip_text(album_count_text)
            .css_classes(["album-artist-label"])
            .build();

        let artist_tile = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .hexpand(false)
            .vexpand(false)
            .spacing(2)
            .css_classes(["album-tile"])
            .build();

        artist_tile.append(&cover_art.widget);
        artist_tile.append(name_label.upcast_ref::<Widget>());
        artist_tile.append(album_count_label.upcast_ref::<Widget>());

        artist_tile.set_accessible_role(Group);
        artist_tile.set_tooltip_text(Some(&artist.name));

        let child = FlowBoxChild::new();
        child.set_child(Some(&artist_tile));
        child.set_focusable(true);

        let click_controller = GestureClick::new();

        if let Some(callback) = on_card_clicked {
            let callback_for_click = callback.clone();
            let callback_for_activate = callback.clone();

            click_controller.connect_released(move |_gesture, _n_press, _x, _y| {
                callback_for_click();
            });

            child.connect_activate(move |_| {
                callback_for_activate();
            });
        }

        artist_tile.add_controller(click_controller);

        Self {
            widget: child.upcast_ref::<Widget>().clone(),
            artist_tile,
            cover_art,
            name_label,
            album_count_label,
        }
    }

    /// Creates a builder for configuring artist cards.
    #[must_use]
    pub fn builder() -> ArtistCardBuilder {
        ArtistCardBuilder::default()
    }

    /// Updates the cover size for this artist card.
    ///
    /// # Arguments
    ///
    /// * `cover_size` - New cover size in pixels
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `UiError` if the size is invalid.
    ///
    /// # Errors
    ///
    /// Returns a `UiError::BuilderError` if the cover size or calculated
    /// `max_width_chars` cannot be converted to i32.
    pub fn update_cover_size(&self, cover_size: u32) -> Result<(), UiError> {
        let cover_size_i32 = i32::try_from(cover_size)
            .map_err(|_| BuilderError(format!("Invalid cover size: {cover_size}")))?;

        self.cover_art
            .update_dimensions(cover_size_i32, cover_size_i32);

        let max_width = ((cover_size - 16) / 10).max(8);
        let max_width_i32 = i32::try_from(max_width)
            .map_err(|_| BuilderError(format!("Invalid max_width_chars: {max_width}")))?;
        self.name_label.set_max_width_chars(max_width_i32);
        self.album_count_label.set_max_width_chars(max_width_i32);
        Ok(())
    }
}

/// Builder pattern for configuring `ArtistCard` components.
#[derive(Default)]
pub struct ArtistCardBuilder {
    /// The artist data to display on the card.
    artist: Option<Artist>,
    /// Optional cover size override in pixels.
    cover_size: Option<u32>,
    /// Optional callback invoked when the card is clicked.
    on_card_clicked: Option<Rc<dyn Fn()>>,
}

impl ArtistCardBuilder {
    /// Sets the artist data for the card.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn artist(mut self, artist: Artist) -> Self {
        self.artist = Some(artist);
        self
    }

    /// Sets the cover size for the artist card.
    ///
    /// # Arguments
    ///
    /// * `cover_size` - The size of the cover art in pixels
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn cover_size(mut self, cover_size: u32) -> Self {
        self.cover_size = Some(cover_size);
        self
    }

    /// Sets the callback for when the card is clicked.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call when card is clicked
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn on_card_clicked<F>(mut self, callback: F) -> Self
    where
        F: Fn() + 'static,
    {
        self.on_card_clicked = Some(Rc::new(callback));
        self
    }

    /// Builds the `ArtistCard` component.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `ArtistCard` instance or a `UiError` if
    /// required fields are missing.
    ///
    /// # Errors
    ///
    /// Returns a `UiError::BuilderError` if the artist field has not been set.
    pub fn build(self) -> Result<ArtistCard, UiError> {
        let artist = self
            .artist
            .ok_or_else(|| BuilderError("Artist must be set".to_string()))?;
        let cover_size = self.cover_size.unwrap_or(180);
        Ok(ArtistCard::new(&artist, cover_size, self.on_card_clicked))
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
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    };

    use crate::{
        error::domain::UiError::BuilderError,
        library::models::Artist,
        ui::views::artist_grid::{ArtistCard, ArtistGridView},
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_creation() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            created_at: None,
            updated_at: None,
        };

        let card = ArtistCard::new(&artist, 180, None);

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_builder() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            created_at: None,
            updated_at: None,
        };
        let clicked = AtomicBool::new(false);
        let clicked = Arc::new(clicked);

        let card = ArtistCard::builder()
            .artist(artist.clone())
            .cover_size(200)
            .on_card_clicked({
                let clicked = clicked.clone();
                move || {
                    clicked.store(true, SeqCst);
                }
            })
            .build()
            .expect("Failed to build ArtistCard");

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_builder_missing_artist() {
        let result = ArtistCard::builder().cover_size(200).build();
        assert!(result.is_err());
        assert!(matches!(result, Err(BuilderError(_))));
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_default_cover_size() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            ..Artist::default()
        };

        let card = ArtistCard::builder()
            .artist(artist)
            .build()
            .expect("Failed to build");

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_update_cover_size() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 180, None);

        card.update_cover_size(250)
            .expect("Failed to update cover size");

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_small_cover_size() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 120, None);

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_large_cover_size() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 400, None);

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_long_name() {
        let artist = Artist {
            id: 1,
            name: "A Very Long Artist Name That Should Be Elided".to_string(),
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 180, None);

        assert_eq!(
            card.name_label.label(),
            "A Very Long Artist Name That Should Be Elided"
        );
        assert_eq!(card.album_count_label.label(), "Albums");
    }

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
        let mut artists = [
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

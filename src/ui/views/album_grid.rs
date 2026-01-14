//! Default album grid view with cover art and metadata.
//!
//! This module implements the `AlbumGridView` component that displays albums
//! in a responsive grid layout with cover art, DR badges, and metadata,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use {
    libadwaita::{
        glib::{JoinHandle, MainContext},
        gtk::{
            AccessibleRole::Grid,
            Align::{Fill, Start},
            Box, FlowBox,
            Orientation::Vertical,
            SelectionMode::None as SelectionNone,
            Widget,
        },
        prelude::{AccessibleExt, BoxExt, Cast, WidgetExt},
    },
    tracing::debug,
};

use crate::{
    audio::{
        decoder::AudioFormat,
        engine::{AudioEngine, PlaybackState::Playing, TrackInfo},
        metadata::TagReader,
    },
    library::{LibraryDatabase, models::Album},
    state::{
        AppState, LibraryState,
        NavigationState::AlbumDetail,
        ZoomEvent::GridZoomChanged,
        app_state::AppStateEvent::{
            MetadataOverlaysChanged, SettingsChanged, YearDisplayModeChanged,
        },
    },
    ui::{
        components::{
            album_card::AlbumCard,
            empty_state::{EmptyState, EmptyStateConfig},
        },
        utils::create_format_display,
    },
};

/// Builder pattern for configuring `AlbumGridView` components.
#[derive(Default)]
pub struct AlbumGridViewBuilder {
    /// Optional application state reference for reactive updates.
    app_state: Option<Arc<AppState>>,
    /// Optional library database reference for fetching tracks.
    library_db: Option<Arc<LibraryDatabase>>,
    /// Optional audio engine reference for playback.
    audio_engine: Option<Arc<AudioEngine>>,
    /// Vector of albums to display in the grid.
    albums: Vec<Album>,
    /// Whether to show DR badges on album covers.
    show_dr_badges: bool,
    /// Whether to use compact layout with smaller cover sizes.
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
    #[must_use]
    pub fn app_state(mut self, app_state: Arc<AppState>) -> Self {
        self.app_state = Some(app_state);
        self
    }

    /// Sets the library database for fetching tracks.
    ///
    /// # Arguments
    ///
    /// * `library_db` - Library database reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn library_db(mut self, library_db: Arc<LibraryDatabase>) -> Self {
        self.library_db = Some(library_db);
        self
    }

    /// Sets the audio engine for playback.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Audio engine reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn audio_engine(mut self, audio_engine: Arc<AudioEngine>) -> Self {
        self.audio_engine = Some(audio_engine);
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Builds the `AlbumGridView` component.
    ///
    /// # Returns
    ///
    /// A new `AlbumGridView` instance.
    #[must_use]
    pub fn build(self) -> AlbumGridView {
        AlbumGridView::new(
            self.app_state.as_ref(),
            self.library_db.as_ref(),
            self.audio_engine.as_ref(),
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
    /// The underlying GTK widget (`FlowBox`).
    pub widget: Widget,
    /// The flow box container.
    pub flow_box: FlowBox,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Library database reference for fetching tracks.
    pub library_db: Option<Arc<LibraryDatabase>>,
    /// Audio engine reference for playback.
    pub audio_engine: Option<Arc<AudioEngine>>,
    /// Current albums being displayed.
    pub albums: Vec<Album>,
    /// Configuration flags.
    pub config: AlbumGridViewConfig,
    /// Empty state component for when no albums are available.
    pub empty_state: Option<EmptyState>,
    /// Current sort criteria.
    pub current_sort: AlbumSortCriteria,
    /// References to album card instances for dynamic updates.
    pub album_cards: Rc<RefCell<Vec<AlbumCard>>>,
    /// Zoom subscription handle for cleanup.
    _zoom_subscription_handle: Option<JoinHandle<()>>,
    /// Settings subscription handle for cleanup.
    _settings_subscription_handle: Option<JoinHandle<()>>,
}

/// Configuration for `AlbumGridView` display options.
#[derive(Debug, Clone)]
pub struct AlbumGridViewConfig {
    /// Whether to show DR badges on album covers.
    pub show_dr_badges: bool,
    /// Whether to use compact layout.
    pub compact: bool,
}

impl AlbumGridView {
    /// Creates a new `AlbumGridView` component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `library_db` - Optional library database reference for fetching tracks
    /// * `audio_engine` - Optional audio engine reference for playback
    /// * `albums` - Initial albums to display
    /// * `show_dr_badges` - Whether to show DR badges on album covers
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `AlbumGridView` instance.
    ///
    /// # Panics
    ///
    /// Panics if cover size or audio format metadata (sample rate, channels, bits per sample) are negative.
    #[must_use]
    pub fn new(
        app_state: Option<&Arc<AppState>>,
        library_db: Option<&Arc<LibraryDatabase>>,
        audio_engine: Option<&Arc<AudioEngine>>,
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
                Some(Arc::clone(state)),
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

        let album_cards = Rc::new(RefCell::new(Vec::new()));

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            flow_box: flow_box.clone(),
            app_state: app_state.cloned(),
            library_db: library_db.cloned(),
            audio_engine: audio_engine.cloned(),
            albums: Vec::new(),
            config: config.clone(),
            empty_state,
            current_sort: AlbumSortCriteria::Title, // Default sort by Title
            album_cards: album_cards.clone(),
            _zoom_subscription_handle: if let Some(state) = app_state {
                // Subscribe to zoom changes
                let state_clone: Arc<AppState> = state.clone();
                let flow_box_clone = flow_box.clone();
                let config_clone = config.clone();
                let album_cards_clone = album_cards.clone();
                let handle = MainContext::default().spawn_local(async move {
                    let rx = state_clone.zoom_manager.subscribe();
                    while let Ok(event) = rx.recv().await {
                        if let GridZoomChanged(_) = event {
                            // Rebuild all album items with new zoom level
                            // Get current library state
                            let library_state = state_clone.get_library_state();

                            // Clear existing children
                            while let Some(child) = flow_box_clone.first_child() {
                                flow_box_clone.remove(&child);
                            }
                            album_cards_clone.borrow_mut().clear();

                            // Add new album items with updated dimensions
                            for album in &library_state.albums {
                                // Look up artist name from app state
                                let artist_name = {
                                    library_state
                                        .artists
                                        .iter()
                                        .find(|artist| artist.id == album.artist_id)
                                        .map_or_else(
                                            || "Unknown Artist".to_string(),
                                            |artist| artist.name.clone(),
                                        )
                                };

                                // Create album card with proper callbacks
                                let format = create_format_display(album).unwrap_or_default();

                                // Get cover size from zoom manager
                                let cover_size =
                                    state_clone.zoom_manager.get_grid_cover_dimensions().0;

                                let show_dr_badge = state_clone
                                    .get_settings_manager()
                                    .read()
                                    .get_settings()
                                    .show_dr_values;

                                let album_card = AlbumCard::builder()
                                    .album(album.clone())
                                    .artist_name(artist_name)
                                    .format(format)
                                    .show_dr_badge(show_dr_badge)
                                    .compact(config_clone.compact)
                                    .cover_size(u32::try_from(cover_size).unwrap())
                                    .on_card_clicked({
                                        let app_state_inner = state_clone.clone();
                                        let album_clone = album.clone();
                                        move || {
                                            app_state_inner.update_navigation(AlbumDetail(
                                                album_clone.clone(),
                                            ));
                                        }
                                    })
                                    .build();

                                album_cards_clone.borrow_mut().push(album_card.clone());
                                flow_box_clone.insert(&album_card.widget, -1);
                            }
                        }
                    }
                });
                Some(handle)
            } else {
                None
            },
            _settings_subscription_handle: if let Some(state) = app_state {
                // Subscribe to settings changes
                let state_clone: Arc<AppState> = state.clone();
                let album_cards_clone = album_cards.clone();
                let handle = MainContext::default().spawn_local(async move {
                    let rx = state_clone.subscribe();
                    while let Ok(event) = rx.recv().await {
                        match event {
                            SettingsChanged { show_dr_values } => {
                                // Update all album cards with new DR badge visibility
                                let mut cards = album_cards_clone.borrow_mut();
                                for card in cards.iter_mut() {
                                    card.update_dr_badge_visibility(show_dr_values);
                                }
                            }
                            MetadataOverlaysChanged { show_overlays } => {
                                // Update all album cards with new metadata overlay visibility
                                let mut cards = album_cards_clone.borrow_mut();
                                for card in cards.iter_mut() {
                                    card.update_metadata_overlay_visibility(show_overlays);
                                }
                            }
                            YearDisplayModeChanged { mode } => {
                                // Update all album cards with new year display mode
                                // For now, this doesn't change anything since we only have release year
                                // In the future, when original_year is implemented, this will update
                                // the year labels to show either release or original year
                                debug!("Year display mode changed to: {}", mode);

                                // Note: This is a placeholder for future implementation
                                // when original_year field is added to the Album model
                            }
                            _ => {}
                        }
                    }
                });
                Some(handle)
            } else {
                None
            },
        };

        // Populate with initial albums
        view.set_albums(albums);

        view
    }

    /// Creates an `AlbumGridView` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `AlbumGridViewBuilder` instance.
    #[must_use]
    pub fn builder() -> AlbumGridViewBuilder {
        AlbumGridViewBuilder::default()
    }

    /// Sets the albums to display in the grid.
    ///
    /// # Arguments
    ///
    /// * `albums` - New vector of albums to display
    ///
    /// # Panics
    ///
    /// Panics if empty state exists but is None (should never happen with proper initialization).
    pub fn set_albums(&mut self, albums: Vec<Album>) {
        // Clear existing children
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        self.albums = albums;
        self.album_cards.borrow_mut().clear();

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
            let album_card = self.create_album_card(album);
            self.flow_box.insert(&album_card.widget, -1);
            self.album_cards.borrow_mut().push(album_card);
        }
    }

    /// Creates a single album card for the grid.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to create a card for
    ///
    /// # Returns
    ///
    /// A new `AlbumCard` instance.
    fn create_album_card(&self, album: &Album) -> AlbumCard {
        // Look up artist name from app state
        let artist_name = if let Some(app_state) = &self.app_state {
            let library_state = app_state.get_library_state();
            library_state
                .artists
                .iter()
                .find(|artist| artist.id == album.artist_id)
                .map_or_else(
                    || "Unknown Artist".to_string(),
                    |artist| artist.name.clone(),
                )
        } else {
            "Unknown Artist".to_string()
        };

        // Create album card with proper callbacks
        // Use the actual format from the album metadata, including bit depth and sample rate
        let format = create_format_display(album).unwrap_or_default();

        // Get cover size from zoom manager if available
        let cover_size = if let Some(app_state) = &self.app_state {
            app_state.zoom_manager.get_grid_cover_dimensions().0
        } else {
            // Default cover size based on compact mode
            if self.config.compact { 120 } else { 180 }
        };

        // Get settings for DR badge and metadata overlays visibility
        let (show_dr_badge, show_metadata_overlays) = if let Some(app_state) = &self.app_state {
            let settings = app_state
                .get_settings_manager()
                .read()
                .get_settings()
                .clone();
            (settings.show_dr_values, settings.show_metadata_overlays)
        } else {
            (self.config.show_dr_badges, true) // Default to showing overlays
        };

        let mut album_card = AlbumCard::builder()
            .album(album.clone())
            .artist_name(artist_name)
            .format(format)
            .show_dr_badge(show_dr_badge)
            .compact(self.config.compact)
            .cover_size(u32::try_from(cover_size).unwrap())
            .on_play_clicked({
                let app_state = self.app_state.clone();
                let library_db = self.library_db.clone();
                let audio_engine = self.audio_engine.clone();
                let album_clone = album.clone();
                move || {
                    // Handle play button click - queue album for playback
                    if let (Some(app_state), Some(library_db), Some(audio_engine)) = (
                        app_state.as_ref(),
                        library_db.as_ref(),
                        audio_engine.as_ref(),
                    ) {
                        let album_id = album_clone.id;
                        let app_state_clone = app_state.clone();
                        let library_db_clone = library_db.clone();
                        let audio_engine_clone = audio_engine.clone();

                        MainContext::default().spawn_local(async move {
                            if let Ok(tracks) = library_db_clone.get_tracks_by_album(album_id).await
                                && !tracks.is_empty()
                            {
                                let first_track = &tracks[0];
                                let track_path = &first_track.path;

                                if let Ok(()) = audio_engine_clone.load_track(track_path)
                                    && let Ok(()) = audio_engine_clone.play().await
                                {
                                    app_state_clone.update_playback_state(Playing);

                                    if let Ok(metadata) = TagReader::read_metadata(track_path) {
                                        let track_info = TrackInfo {
                                            path: track_path.clone(),
                                            metadata,
                                            format: AudioFormat {
                                                sample_rate: u32::try_from(first_track.sample_rate)
                                                    .unwrap_or(44100),
                                                channels: u32::try_from(first_track.channels)
                                                    .unwrap_or(2),
                                                bits_per_sample: u32::try_from(
                                                    first_track.bits_per_sample,
                                                )
                                                .unwrap_or(16),
                                                channel_mask: 0,
                                            },
                                            duration_ms: u64::try_from(first_track.duration_ms)
                                                .unwrap_or(0),
                                        };
                                        app_state_clone.update_current_track(Some(track_info));
                                    }
                                }
                            }
                        });
                    }
                }
            })
            .on_card_clicked({
                let app_state = self.app_state.clone();
                let album_clone = album.clone();
                move || {
                    // Handle card click - navigate to detail view
                    if let Some(state) = &app_state {
                        state.update_navigation(AlbumDetail(album_clone.clone()));
                    }
                }
            })
            .build();

        // Apply metadata overlay visibility setting
        album_card.update_metadata_overlay_visibility(show_metadata_overlays);

        album_card
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

    /// Updates the DR badge visibility setting for this view.
    ///
    /// # Arguments
    ///
    /// * `show_dr_badges` - Whether to show DR badges
    pub fn set_show_dr_badges(&mut self, show_dr_badges: bool) {
        self.config.show_dr_badges = show_dr_badges;

        // Update all existing album cards directly
        for album_card in self.album_cards.borrow_mut().iter_mut() {
            album_card.update_dr_badge_visibility(show_dr_badges);
        }
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
        Self::new(None, None, None, Vec::new(), true, false)
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
                format: Some("FLAC".to_string()),
                bits_per_sample: Some(24),
                sample_rate: Some(96000),
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
                format: Some("WAV".to_string()),
                bits_per_sample: Some(16),
                sample_rate: Some(44100),
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
        let mut albums = [
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

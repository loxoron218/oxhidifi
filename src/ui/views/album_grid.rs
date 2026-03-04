//! Default album grid view with cover art and metadata.
//!
//! This module implements the `AlbumGridView` component that displays albums
//! in a responsive grid layout with cover art, DR badges, and metadata,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::{cell::RefCell, collections::HashSet, rc::Rc, sync::Arc};

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
        prelude::{AccessibleExt, BoxExt, Cast, ObjectExt, WidgetExt},
    },
    tracing::{debug, error, warn},
};

use crate::{
    audio::{
        engine::{AudioEngine, PlaybackState::Playing},
        queue_manager::QueueManager,
    },
    error::domain::UiError,
    library::{database::LibraryDatabase, models::Album},
    state::{
        app_state::{
            AppState,
            AppStateEvent::{
                CurrentTrackChanged, MetadataOverlaysChanged, PlaybackStateChanged, QueueChanged,
                SettingsChanged, YearDisplayModeChanged,
            },
            LibraryState,
            NavigationState::AlbumDetail,
        },
        zoom_manager::ZoomEvent::GridZoomChanged,
    },
    ui::{
        components::{
            album_card::AlbumCard,
            empty_state::{EmptyState, EmptyStateConfig},
            search_empty_state::SearchEmptyState,
        },
        formatting::create_format_display,
        views::{detail_playback::play_album, filtering::Filterable},
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
    /// Optional queue manager reference for queue operations.
    queue_manager: Option<Arc<QueueManager>>,
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

    /// Sets the queue manager reference for queue operations.
    ///
    /// # Arguments
    ///
    /// * `queue_manager` - Queue manager reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn queue_manager(mut self, queue_manager: Arc<QueueManager>) -> Self {
        self.queue_manager = Some(queue_manager);
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
            self.queue_manager.as_ref(),
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
    /// Queue manager reference for queue operations.
    pub queue_manager: Option<Arc<QueueManager>>,
    /// Current albums being displayed.
    pub albums: Vec<Album>,
    /// Full unfiltered list of all albums.
    pub all_albums: Vec<Album>,
    /// Configuration flags.
    pub config: AlbumGridViewConfig,
    /// Empty state component for when no albums are available.
    pub empty_state: Option<EmptyState>,
    /// Search empty state component for when search returns no results.
    pub search_empty_state: SearchEmptyState,
    /// Current sort criteria.
    pub current_sort: AlbumSortCriteria,
    /// References to album card instances for dynamic updates.
    pub album_cards: Rc<RefCell<Vec<AlbumCard>>>,
    /// Zoom subscription handle for cleanup.
    zoom_subscription_handle: Option<JoinHandle<()>>,
    /// Settings subscription handle for cleanup.
    settings_subscription_handle: Option<JoinHandle<()>>,
    /// Playback subscription handle for cleanup.
    playback_subscription_handle: Option<JoinHandle<()>>,
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
    /// * `queue_manager` - Optional queue manager reference for queue operations
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
        queue_manager: Option<&Arc<QueueManager>>,
        albums: Vec<Album>,
        show_dr_badges: bool,
        compact: bool,
    ) -> Self {
        let config = AlbumGridViewConfig {
            show_dr_badges,
            compact,
        };

        let flow_box = Self::create_flow_box();

        let (main_container, empty_state, search_empty_state) =
            Self::create_main_container(&flow_box, app_state);

        let album_cards = Rc::new(RefCell::new(Vec::new()));

        let zoom_subscription_handle =
            Self::create_zoom_subscription(app_state, &flow_box, &config, &album_cards);

        let settings_subscription_handle =
            Self::create_settings_subscription(app_state, &album_cards);

        let playback_subscription_handle =
            Self::create_playback_subscription(app_state, &album_cards);

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            flow_box,
            app_state: app_state.cloned(),
            library_db: library_db.cloned(),
            audio_engine: audio_engine.cloned(),
            queue_manager: queue_manager.cloned(),
            albums: Vec::new(),
            all_albums: albums.clone(),
            config,
            empty_state,
            search_empty_state,
            current_sort: AlbumSortCriteria::Title,
            album_cards,
            zoom_subscription_handle,
            settings_subscription_handle,
            playback_subscription_handle,
        };

        view.set_albums(albums);

        view
    }

    /// Creates the flow box widget for the grid layout.
    ///
    /// # Returns
    ///
    /// A configured `FlowBox` widget.
    fn create_flow_box() -> FlowBox {
        FlowBox::builder()
            .halign(Fill)
            .valign(Start)
            .width_request(360)
            .homogeneous(true)
            .max_children_per_line(100)
            .selection_mode(SelectionNone)
            .row_spacing(6)
            .column_spacing(6)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .hexpand(true)
            .vexpand(false)
            .css_classes(["album-grid"])
            .build()
    }

    /// Creates the main container and empty state components.
    ///
    /// # Arguments
    ///
    /// * `flow_box` - The flow box widget to add to the container
    /// * `app_state` - Optional application state reference
    ///
    /// # Returns
    ///
    /// A tuple of (`main_container`, `empty_state`, `search_empty_state`).
    fn create_main_container(
        flow_box: &FlowBox,
        app_state: Option<&Arc<AppState>>,
    ) -> (Box, Option<EmptyState>, SearchEmptyState) {
        let main_container = Box::builder().orientation(Vertical).build();

        main_container.append(&flow_box.clone().upcast::<Widget>());

        // Set ARIA attributes for accessibility
        flow_box.set_accessible_role(Grid);

        // Create empty state component
        let empty_state = app_state.as_ref().map(|state| {
            EmptyState::new(
                Some(Arc::clone(state)),
                None,
                EmptyStateConfig {
                    is_album_view: true,
                },
                None,
            )
        });

        // Add empty state to main container if it exists
        if let Some(empty_state) = &empty_state {
            main_container.append(&empty_state.widget);
        }

        // Create and add search empty state component
        let search_empty_state = SearchEmptyState::builder().is_album_view(true).build();
        main_container.append(search_empty_state.widget());
        search_empty_state.hide();

        (main_container, empty_state, search_empty_state)
    }

    /// Creates the zoom subscription handler.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    /// * `flow_box` - The flow box widget
    /// * `config` - The album grid view configuration
    /// * `album_cards` - The album cards reference
    ///
    /// # Returns
    ///
    /// An optional join handle for the subscription.
    fn create_zoom_subscription(
        app_state: Option<&Arc<AppState>>,
        flow_box: &FlowBox,
        config: &AlbumGridViewConfig,
        album_cards: &Rc<RefCell<Vec<AlbumCard>>>,
    ) -> Option<JoinHandle<()>> {
        app_state.map(|state| {
            let state_clone = state.clone();
            let flow_box_clone = flow_box.clone();
            let config_clone = config.clone();
            let album_cards_clone = album_cards.clone();
            MainContext::default().spawn_local(async move {
                let rx = state_clone.zoom_manager.subscribe();
                while let Ok(event) = rx.recv().await {
                    if let GridZoomChanged(_) = event {
                        // Rebuild all album items with new zoom level
                        // Get current library state
                        let library_state = state_clone.get_library_state();
                        let cover_size_i32 = state_clone.zoom_manager.get_grid_cover_dimensions().0;
                        let show_dr_badge = state_clone
                            .get_settings_manager()
                            .read()
                            .get_settings()
                            .show_dr_values;

                        let mut cards = album_cards_clone.borrow_mut();

                        if cards.len() == library_state.albums.len() {
                            // Album list unchanged, just update existing cards
                            for (album, card) in library_state.albums.iter().zip(cards.iter_mut()) {
                                if album.id == card.album_id {
                                    let size = Self::cover_size_to_u32(cover_size_i32);
                                    card.cover_art.update_dimensions(
                                        i32::try_from(size).unwrap_or(cover_size_i32),
                                        i32::try_from(size).unwrap_or(cover_size_i32),
                                    );
                                    card.update_dr_badge_visibility(show_dr_badge);
                                    if let Err(e) = card.update_label_max_width_chars(size) {
                                        error!(error = %e, "Failed to update label max width chars for album {}", album.title);
                                    }
                                }
                            }
                        } else {
                            // Album list changed, need to rebuild
                            drop(cards);

                            // Clear existing children
                            while let Some(child) = flow_box_clone.first_child() {
                                flow_box_clone.remove(&child);
                            }
                            album_cards_clone.borrow_mut().clear();

                            // Add new album items with updated dimensions
                            for album in &library_state.albums {
                                // Look up artist name from app state
                                let artist_name = library_state
                                    .artists
                                    .iter()
                                    .find(|artist| artist.id == album.artist_id)
                                    .map_or_else(
                                        || "Unknown Artist".to_string(),
                                        |artist| artist.name.clone(),
                                    );

                                // Create album card with proper callbacks
                                let format = create_format_display(album).unwrap_or_default();

                                let album_card = match AlbumCard::builder()
                                    .album(album.clone())
                                    .artist_name(artist_name)
                                    .format(format)
                                    .show_dr_badge(show_dr_badge)
                                    .compact(config_clone.compact)
                                    .cover_size(Self::cover_size_to_u32(cover_size_i32))
                                    .on_card_clicked({
                                        let app_state_inner = state_clone.clone();
                                        let album_clone = album.clone();
                                        move || {
                                            app_state_inner.update_navigation(AlbumDetail(
                                                album_clone.clone(),
                                            ));
                                        }
                                    })
                                    .build() {
                                    Ok(card) => card,
                                    Err(e) => {
                                        error!(error = %e, album_id = album.id, "Failed to build album card in zoom subscription");
                                        continue;
                                    }
                                };

                                album_cards_clone.borrow_mut().push(album_card.clone());
                                flow_box_clone.insert(&album_card.widget, -1);
                            }
                        }
                    }
                }
            })
        })
    }

    /// Creates the settings subscription handler.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    /// * `album_cards` - The album cards reference
    ///
    /// # Returns
    ///
    /// An optional join handle for the subscription.
    fn create_settings_subscription(
        app_state: Option<&Arc<AppState>>,
        album_cards: &Rc<RefCell<Vec<AlbumCard>>>,
    ) -> Option<JoinHandle<()>> {
        app_state.map(|state| {
            let state_clone = state.clone();
            let album_cards_clone = album_cards.clone();
            MainContext::default().spawn_local(async move {
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
                        }
                        _ => {}
                    }
                }
            })
        })
    }

    /// Creates the playback subscription handler.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    /// * `album_cards` - The album cards reference
    ///
    /// # Returns
    ///
    /// An optional join handle for the subscription.
    fn create_playback_subscription(
        app_state: Option<&Arc<AppState>>,
        album_cards: &Rc<RefCell<Vec<AlbumCard>>>,
    ) -> Option<JoinHandle<()>> {
        app_state.map(|state| {
            let state_clone = state.clone();
            let album_cards_clone = album_cards.clone();
            MainContext::default().spawn_local(async move {
                let rx = state_clone.subscribe();
                while let Ok(event) = rx.recv().await {
                    match event {
                        CurrentTrackChanged(_) | PlaybackStateChanged(_) | QueueChanged(_) => {
                            let is_playing = state_clone.get_playback_state() == Playing;
                            let album_id = state_clone.get_current_album_id();

                            let mut cards = album_cards_clone.borrow_mut();
                            if let Some(current_id) = album_id {
                                for card in cards.iter_mut() {
                                    let is_current_album = current_id == card.album_id;
                                    card.set_playing(is_current_album && is_playing);
                                }
                            } else {
                                for card in cards.iter_mut() {
                                    card.set_playing(false);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            })
        })
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
        // Check if albums are actually different to avoid unnecessary widget recreation
        let albums_unchanged = self.albums == albums;

        if albums_unchanged {
            return;
        }

        // Clear existing children
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        self.albums = albums;

        self.album_cards.borrow_mut().clear();

        // Apply current sort
        self.apply_sort();

        // Update empty state visibility only when albums change
        if let Some(empty_state) = &self.empty_state {
            let library_state = if let Some(app_state) = &self.app_state {
                app_state.get_library_state()
            } else {
                LibraryState {
                    albums: self.albums.clone(),
                    ..Default::default()
                }
            };
            empty_state.update_from_library_state(&library_state);
        }

        // Add new album items using the new AlbumCard component
        for album in &self.albums {
            match self.create_album_card(album) {
                Ok(album_card) => {
                    self.flow_box.insert(&album_card.widget, -1);
                    self.album_cards.borrow_mut().push(album_card);
                }
                Err(e) => {
                    error!(error = %e, album_id = album.id, "Failed to create album card");
                }
            }
        }

        // Hide search empty state when showing albums
        self.search_empty_state.hide();
    }

    /// Updates the full unfiltered albums list.
    ///
    /// This should be called when library data changes.
    ///
    /// # Arguments
    ///
    /// * `all_albums` - Complete list of all albums from database
    pub fn update_all_albums(&mut self, all_albums: Vec<Album>) {
        self.all_albums = all_albums;

        // If there's no active search filter, show all albums
        let library_state = self.app_state.as_ref().map(|s| s.get_library_state());
        if library_state
            .as_ref()
            .and_then(|s| s.search_filter.as_ref())
            .is_none_or(String::is_empty)
        {
            self.set_albums(self.all_albums.clone());
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
    /// A `Result` containing the new `AlbumCard` instance or an error.
    ///
    /// # Errors
    ///
    /// Returns `UiError` if the album card creation fails.
    fn create_album_card(&self, album: &Album) -> Result<AlbumCard, UiError> {
        let artist_name = self.resolve_artist_name(album);

        let format = create_format_display(album).unwrap_or_default();

        let cover_size_i32 = self.get_cover_size();

        let (show_dr_badge, show_metadata_overlays) = self.get_visibility_settings();

        let mut album_card = AlbumCard::builder()
            .album(album.clone())
            .artist_name(artist_name)
            .format(format)
            .show_dr_badge(show_dr_badge)
            .compact(self.config.compact)
            .cover_size(Self::cover_size_to_u32(cover_size_i32))
            .on_play_clicked(self.create_play_callback(album))
            .on_card_clicked(self.create_card_click_callback(album))
            .build()?;

        // Apply metadata overlay visibility setting
        album_card.update_metadata_overlay_visibility(show_metadata_overlays);

        Ok(album_card)
    }

    /// Resolves the artist name for an album.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to resolve the artist for
    ///
    /// # Returns
    ///
    /// The artist name, or "Unknown Artist" if not found.
    fn resolve_artist_name(&self, album: &Album) -> String {
        self.app_state.as_ref().map_or_else(
            || "Unknown Artist".to_string(),
            |app_state| {
                let library_state = app_state.get_library_state();
                library_state
                    .artists
                    .iter()
                    .find(|artist| artist.id == album.artist_id)
                    .map_or_else(
                        || "Unknown Artist".to_string(),
                        |artist| artist.name.clone(),
                    )
            },
        )
    }

    /// Gets the cover size for album cards.
    ///
    /// # Returns
    ///
    /// The cover size in pixels.
    fn get_cover_size(&self) -> i32 {
        self.app_state
            .as_ref()
            .map_or(if self.config.compact { 120 } else { 180 }, |app_state| {
                app_state.zoom_manager.get_grid_cover_dimensions().0
            })
    }

    /// Gets visibility settings for DR badges and metadata overlays.
    ///
    /// # Returns
    ///
    /// A tuple of (`show_dr_badge`, `show_metadata_overlays`).
    fn get_visibility_settings(&self) -> (bool, bool) {
        self.app_state
            .as_ref()
            .map_or((self.config.show_dr_badges, true), |app_state| {
                let settings = app_state
                    .get_settings_manager()
                    .read()
                    .get_settings()
                    .clone();
                (settings.show_dr_values, settings.show_metadata_overlays)
            })
    }

    /// Converts cover size from i32 to u32 with fallback.
    ///
    /// # Arguments
    ///
    /// * `size` - Cover size as i32
    ///
    /// # Returns
    ///
    /// Cover size as u32, falling back to 180 if negative or out of range.
    fn cover_size_to_u32(size: i32) -> u32 {
        if size <= 0 {
            warn!(cover_size = size, "Invalid cover size, using fallback");
            180
        } else {
            u32::try_from(size).unwrap_or(180)
        }
    }

    /// Creates the play button callback for an album.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to play
    ///
    /// # Returns
    ///
    /// A callback function that plays the album.
    fn create_play_callback(&self, album: &Album) -> impl Fn() + 'static {
        let album_id = album.id;

        let app_state = self.app_state.clone();
        let library_db = self.library_db.clone();
        let audio_engine = self.audio_engine.clone();
        let queue_manager = self.queue_manager.clone();

        move || {
            if let (Some(app_state), Some(library_db), Some(audio_engine), Some(queue_manager)) = (
                app_state.as_ref(),
                library_db.as_ref(),
                audio_engine.as_ref(),
                queue_manager.as_ref(),
            ) {
                let app_state_clone = app_state.clone();
                let library_db_clone = library_db.clone();
                let audio_engine_clone = audio_engine.clone();
                let queue_manager_clone = queue_manager.clone();

                MainContext::default().spawn_local(async move {
                    play_album(
                        album_id,
                        Some(library_db_clone),
                        Some(audio_engine_clone),
                        Some(queue_manager_clone),
                        Some(app_state_clone),
                    )
                    .await;
                });
            }
        }
    }

    /// Creates the card click callback for navigation.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to navigate to
    ///
    /// # Returns
    ///
    /// A callback function that navigates to the album detail view.
    fn create_card_click_callback(&self, album: &Album) -> impl Fn() + 'static {
        let app_state = self.app_state.clone();
        let album_clone = album.clone();

        move || {
            if let Some(state) = &app_state {
                state.update_navigation(AlbumDetail(album_clone.clone()));
            }
        }
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: AlbumGridViewConfig) {
        self.config = config;

        // Rebuild all album items with new configuration
        self.set_albums(self.all_albums.clone());
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
        let all_albums = self.all_albums.clone();

        // Call filter_items to update the grid and get result status
        let has_results = self.filter_items(query, &all_albums, |album, q| {
            album.title.to_lowercase().contains(q)
                || album.artist_id.to_string().to_lowercase().contains(q)
        });

        // Update search empty state visibility
        if has_results {
            self.search_empty_state.hide();
        } else {
            self.search_empty_state.update_search_query(query);
            self.search_empty_state.show();
        }
    }

    /// Clears the view by hiding all items.
    ///
    /// This is used when switching tabs with an active search to prevent
    /// the unfiltered view from appearing during the transition.
    pub fn clear_view(&self) {
        Filterable::<Album>::clear_view(self);
    }
}

impl Filterable<Album> for AlbumGridView {
    /// Returns the unique identifier for an album item.
    ///
    /// # Arguments
    ///
    /// * `item` - The album to get the ID from
    ///
    /// # Returns
    ///
    /// The album's unique identifier.
    fn get_widget_id(&self, item: &Album) -> i64 {
        item.id
    }

    /// Returns a copy of the currently displayed albums.
    ///
    /// # Returns
    ///
    /// A vector of albums currently displayed in the view.
    fn get_current_items(&self) -> Vec<Album> {
        self.albums.clone()
    }

    /// Updates the albums currently displayed in the view.
    ///
    /// # Arguments
    ///
    /// * `items` - New vector of albums to display
    fn set_current_items(&mut self, items: Vec<Album>) {
        self.albums = items;
    }

    /// Sets the visibility of album cards based on filtered IDs.
    ///
    /// # Arguments
    ///
    /// * `visible_ids` - Set of album IDs that should be visible
    fn set_visibility(&self, visible_ids: &HashSet<i64>) {
        let _freeze_guard = self.flow_box.freeze_notify();

        let cards = self.album_cards.borrow();
        for card in cards.iter() {
            let card_visible = visible_ids.contains(&card.album_id);
            card.widget.set_visible(card_visible);
        }
    }
}

impl AlbumGridView {
    /// Cleans up subscription handles.
    pub fn cleanup(&mut self) {
        if let Some(handle) = self.zoom_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.settings_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.playback_subscription_handle.take() {
            handle.abort();
        }
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

        let _freeze_guard = self.flow_box.freeze_notify();

        let mut album_cards = self.album_cards.borrow_mut();

        // Sort album_cards to match the new album order
        album_cards.sort_by(|a, b| {
            self.albums
                .iter()
                .position(|album| album.id == a.album_id)
                .unwrap_or(usize::MAX)
                .cmp(
                    &self
                        .albums
                        .iter()
                        .position(|album| album.id == b.album_id)
                        .unwrap_or(usize::MAX),
                )
        });

        // Remove all children from flow_box
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        // Re-insert children in the sorted order
        for card in album_cards.iter() {
            self.flow_box.insert(&card.widget, -1);
        }
    }

    /// Applies the current sort criteria to the albums vector.
    fn apply_sort(&mut self) {
        match self.current_sort {
            AlbumSortCriteria::Title => {
                self.albums.sort_by(|a, b| a.title.cmp(&b.title));
            }
            AlbumSortCriteria::Artist => {
                self.albums.sort_by_key(|a| a.artist_id);
            }
            AlbumSortCriteria::Year => {
                self.albums.sort_by_key(|a| a.year.unwrap_or(0));
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
        Self::new(None, None, None, None, Vec::new(), true, false)
    }
}

impl Drop for AlbumGridView {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        audio::constants::{DEFAULT_BIT_DEPTH, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE},
        library::models::Album,
        ui::views::album_grid::AlbumGridView,
    };

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
                track_count: 12,
                channels: Some(i64::from(DEFAULT_CHANNELS)),
            },
            Album {
                id: 2,
                artist_id: 2,
                title: "Test Album 2".to_string(),
                year: Some(2022),
                genre: Some("Jazz".to_string()),
                format: Some("WAV".to_string()),
                bits_per_sample: Some(i64::from(DEFAULT_BIT_DEPTH)),
                sample_rate: Some(i64::from(DEFAULT_SAMPLE_RATE)),
                compilation: true,
                path: "/path/to/album2".to_string(),
                dr_value: Some("DR8".to_string()),
                artwork_path: None,
                created_at: None,
                updated_at: None,
                track_count: 8,
                channels: Some(i64::from(DEFAULT_CHANNELS)),
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
        albums.sort_by_key(|a| a.year.unwrap_or(0));
        assert_eq!(albums[0].year, Some(2022));
    }
}

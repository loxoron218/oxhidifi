//! Global application state with reactive update mechanisms.
//!
//! This module provides the central `AppState` container that manages
//! shared state across the application with thread-safe access and
//! reactive update notifications.

use std::{
    collections::HashSet,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::glib::MainContext,
    parking_lot::RwLock,
    tokio::sync::RwLock as TokioRwLock,
    tracing::{debug, error, warn},
};

use crate::{
    audio::engine::{
        AudioEngine,
        PlaybackState::{self, Stopped},
        TrackInfo,
    },
    config::settings::{AlbumGridSortItem, ArtistGridSortItem, SettingsManager},
    library::{
        models::{Album, Artist, Track},
        scanner::LibraryScanner,
    },
    state::zoom_manager::ZoomManager,
};

/// Playback queue state.
#[derive(Debug, Clone, Default)]
pub struct PlaybackQueue {
    /// Tracks in the queue.
    pub tracks: Vec<Track>,
    /// Current track index (if any).
    pub current_index: Option<usize>,
}

/// Central state container with thread-safe access.
///
/// The `AppState` holds all global application state and provides
/// reactive update mechanisms for UI components to subscribe to changes.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current playback information and controls.
    pub playback: Arc<RwLock<PlaybackState>>,
    /// Currently loaded track information.
    pub current_track: Arc<RwLock<Option<TrackInfo>>>,
    /// Currently playing album ID (if any).
    pub current_album_id: Arc<RwLock<Option<i64>>>,
    /// Current library view state.
    pub library: Arc<RwLock<LibraryState>>,
    /// Current navigation state.
    pub navigation: Arc<RwLock<NavigationState>>,
    /// Audio engine reference.
    pub audio_engine: Weak<AudioEngine>,
    /// Library scanner reference (optional).
    pub library_scanner: Arc<RwLock<Option<Arc<TokioRwLock<LibraryScanner>>>>>,
    /// List of active subscribers for manual broadcast fan-out.
    /// We use `async_channel` to avoid Tokio runtime dependencies in the waker logic.
    subscribers: Arc<RwLock<Vec<Sender<Arc<AppStateEvent>>>>>,
    /// Zoom manager for handling view zoom levels.
    pub zoom_manager: Arc<ZoomManager>,
    /// Settings manager reference for persistence (wrapped in `RwLock` for mutability).
    pub settings_manager: Arc<RwLock<SettingsManager>>,
    /// Global scanning state - true when library is being scanned.
    pub is_scanning: Arc<AtomicBool>,
}

/// Current library view state.
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    /// Currently displayed albums.
    pub albums: Vec<Album>,
    /// Currently displayed artists.
    pub artists: Vec<Artist>,
    /// Currently selected album (if any).
    pub selected_album: Option<Album>,
    /// Currently selected artist (if any).
    pub selected_artist: Option<Artist>,
    /// Currently playing tracks (if any).
    pub current_tracks: Vec<Track>,
    /// Current search filter.
    pub search_filter: Option<String>,
    /// Current view mode (grid/list).
    pub view_mode: ViewMode,
    /// Currently selected tab (albums or artists).
    pub current_tab: LibraryTab,
    /// Set of selected album IDs (for multi-selection).
    pub selected_album_ids: HashSet<i64>,
    /// Set of selected artist IDs (for multi-selection).
    pub selected_artist_ids: HashSet<i64>,
}

impl LibraryState {
    /// Returns selection info for the current tab.
    ///
    /// # Returns
    ///
    /// A tuple of (`selected_count`, `total_count`, `item_type`)
    #[must_use]
    pub fn current_selection(&self) -> (usize, usize, &'static str) {
        match self.current_tab {
            LibraryTab::Albums => (self.selected_album_ids.len(), self.albums.len(), "album"),
            LibraryTab::Artists => (self.selected_artist_ids.len(), self.artists.len(), "artist"),
        }
    }
}

/// Navigation state tracking.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NavigationState {
    /// Main library view (root).
    #[default]
    Library,
    /// Album detail view.
    AlbumDetail(Album),
    /// Artist detail view.
    ArtistDetail(Artist),
}

/// Library tab selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LibraryTab {
    /// Albums tab is selected (default).
    #[default]
    Albums,
    /// Artists tab is selected.
    Artists,
}

/// View mode for library display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Grid view (default).
    #[default]
    Grid,
    /// List/column view.
    List,
}

/// Application state change events.
#[derive(Debug, Clone)]
pub enum AppStateEvent {
    /// Playback state changed.
    PlaybackStateChanged(PlaybackState),
    /// Current track changed.
    CurrentTrackChanged(Box<Option<TrackInfo>>),
    /// Library data (content) changed.
    LibraryDataChanged {
        albums: Vec<Album>,
        artists: Vec<Artist>,
    },
    /// Navigation state changed.
    NavigationChanged(Box<NavigationState>),
    /// View options changed (tab/mode).
    ViewOptionsChanged {
        current_tab: LibraryTab,
        view_mode: ViewMode,
    },
    /// Search filter changed.
    SearchFilterChanged(Option<String>),
    /// Selection changed (albums or artists).
    SelectionChanged {
        tab: LibraryTab,
        selected_ids: HashSet<i64>,
    },
    /// Grid sort configuration changed.
    GridSortChanged(LibraryTab),
    /// User settings changed that affect UI display.
    SettingsChanged { show_dr_values: bool },
    /// Metadata overlays visibility setting changed.
    MetadataOverlaysChanged { show_overlays: bool },
    /// Year display mode setting changed.
    YearDisplayModeChanged { mode: String },
    /// Playback queue changed.
    QueueChanged(PlaybackQueue),
    /// Exclusive mode setting changed.
    ExclusiveModeChanged { enabled: bool },
    /// Exclusive mode playback failed.
    ExclusiveModeFailed { reason: String },
    /// Library scan failed.
    LibraryScanFailed { reason: String },
    /// Library scanning state changed.
    LibraryScanningChanged { is_scanning: bool },
    /// Library scan progress update.
    LibraryScanProgress {
        /// Number of albums processed so far.
        current: usize,
        /// Total number of albums to process.
        total: usize,
    },
}

impl AppState {
    /// Creates a new application state instance.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Reference to the audio engine.
    /// * `library_scanner` - Optional library scanner reference.
    /// * `settings_manager` - Settings manager reference for zoom persistence.
    ///
    /// # Returns
    ///
    /// A new `AppState` instance.
    pub fn new(
        audio_engine: Weak<AudioEngine>,
        library_scanner: Option<Arc<TokioRwLock<LibraryScanner>>>,
        settings_manager: Arc<RwLock<SettingsManager>>,
    ) -> Self {
        let zoom_manager = Arc::new(ZoomManager::new(Arc::clone(&settings_manager)));

        let state = Self {
            playback: Arc::new(RwLock::new(Stopped)),
            current_track: Arc::new(RwLock::new(None)),
            current_album_id: Arc::new(RwLock::new(None)),
            library: Arc::new(RwLock::new(LibraryState::default())),
            navigation: Arc::new(RwLock::new(NavigationState::default())),
            audio_engine,
            library_scanner: Arc::new(RwLock::new(library_scanner)),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            zoom_manager,
            settings_manager,
            is_scanning: Arc::new(AtomicBool::new(false)),
        };

        state.listen_to_audio_engine();
        state
    }

    /// Listens to audio engine state changes and forwards them to subscribers.
    fn listen_to_audio_engine(&self) {
        if let Some(audio_engine) = self.audio_engine.upgrade() {
            let subscribers = Arc::clone(&self.subscribers);
            let playback_state = Arc::clone(&self.playback);
            let receiver = audio_engine.subscribe_to_state_changes();

            MainContext::default().spawn_local(async move {
                while let Ok(state) = receiver.recv().await {
                    *playback_state.write() = state.clone();
                    let current_subscribers = subscribers.read();
                    let event = Arc::new(AppStateEvent::PlaybackStateChanged(state));
                    for tx in current_subscribers.iter() {
                        if let Err(e) = tx.try_send(Arc::clone(&event)) {
                            error!(error = %e, "Failed to send playback state event to subscriber");
                        }
                    }
                }
            });
        }
    }

    /// Helper to broadcast an event to all subscribers.
    /// Cleans up closed channels.
    fn broadcast_event(&self, event: &AppStateEvent) -> usize {
        let mut subscribers = self.subscribers.write();
        let event = Arc::new(event.clone());

        subscribers.retain(|tx| {
            if tx.is_closed() {
                false
            } else {
                if tx.try_send(Arc::clone(&event)).is_err() {
                    warn!(
                        channel = "event_subscriber",
                        "Failed to send event to subscriber, but retaining channel"
                    );
                }
                true
            }
        });

        subscribers.len()
    }

    /// Helper to broadcast selection change event.
    fn broadcast_selection_change(&self, tab: LibraryTab) {
        let selected_ids = match tab {
            LibraryTab::Albums => self.library.read().selected_album_ids.clone(),
            LibraryTab::Artists => self.library.read().selected_artist_ids.clone(),
        };
        self.broadcast_event(&AppStateEvent::SelectionChanged { tab, selected_ids });
    }

    /// Updates the playback state and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `state` - New playback state.
    pub fn update_playback_state(&self, state: PlaybackState) {
        debug!("AppState: Updating playback state to {:?}", state);
        *self.playback.write() = state.clone();
        self.broadcast_event(&AppStateEvent::PlaybackStateChanged(state));
    }

    /// Updates the current track and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `track` - New current track information.
    pub fn update_current_track(&self, track: Option<TrackInfo>) {
        (*self.current_track.write()).clone_from(&track);
        self.broadcast_event(&AppStateEvent::CurrentTrackChanged(Box::new(track)));
    }

    /// Updates only the library data (albums/artists) without changing navigation.
    ///
    /// # Arguments
    ///
    /// * `albums` - New albums list
    /// * `artists` - New artists list
    pub fn update_library_data(&self, albums: Vec<Album>, artists: Vec<Artist>) {
        debug!(
            "AppState: Updating library data - {} albums, {} artists",
            albums.len(),
            artists.len()
        );

        {
            let mut library = self.library.write();
            library.albums.clone_from(&albums);
            library.artists.clone_from(&artists);
        }

        self.broadcast_event(&AppStateEvent::LibraryDataChanged { albums, artists });
    }

    /// Updates the navigation stack state.
    ///
    /// # Arguments
    ///
    /// * `state` - New navigation state
    pub fn update_navigation(&self, state: NavigationState) {
        let changed = {
            let mut nav = self.navigation.write();
            if *nav == state {
                false
            } else {
                debug!("AppState: Updating navigation to {:?}", state);
                *nav = state.clone();
                true
            }
        };

        if changed {
            self.broadcast_event(&AppStateEvent::NavigationChanged(Box::new(state)));
        }
    }

    /// Updates only the view options (tab/view mode) without changing main navigation.
    ///
    /// # Arguments
    ///
    /// * `current_tab` - New tab
    /// * `view_mode` - New view mode
    pub fn update_view_options(&self, current_tab: LibraryTab, view_mode: ViewMode) {
        let changed = {
            let mut library = self.library.write();
            if library.current_tab != current_tab || library.view_mode != view_mode {
                debug!(
                    "AppState: Updating view options - tab={:?}, view_mode={:?}",
                    current_tab, view_mode
                );
                library.current_tab = current_tab.clone();
                library.view_mode = view_mode.clone();
                true
            } else {
                false
            }
        };

        if changed {
            self.broadcast_event(&AppStateEvent::ViewOptionsChanged {
                current_tab,
                view_mode,
            });
        }
    }

    /// Toggles album selection by ID.
    ///
    /// # Arguments
    ///
    /// * `album_id` - Album ID to toggle
    ///
    /// # Returns
    ///
    /// `true` if the album is now selected, `false` if deselected
    #[must_use]
    pub fn toggle_album_selection(&self, album_id: i64) -> bool {
        let mut library = self.library.write();
        if library.selected_album_ids.contains(&album_id) {
            library.selected_album_ids.remove(&album_id);
            false
        } else {
            library.selected_album_ids.insert(album_id);
            true
        }
    }

    /// Selects an album by ID.
    ///
    /// # Arguments
    ///
    /// * `album_id` - Album ID to select
    pub fn select_album(&self, album_id: i64) {
        self.library.write().selected_album_ids.insert(album_id);
        self.broadcast_selection_change(LibraryTab::Albums);
    }

    /// Deselects an album by ID.
    ///
    /// # Arguments
    ///
    /// * `album_id` - Album ID to deselect
    pub fn deselect_album(&self, album_id: i64) {
        self.library.write().selected_album_ids.remove(&album_id);
        self.broadcast_selection_change(LibraryTab::Albums);
    }

    /// Clears all album selections.
    pub fn clear_album_selection(&self) {
        self.library.write().selected_album_ids.clear();
        self.broadcast_selection_change(LibraryTab::Albums);
    }

    /// Gets all selected album IDs.
    ///
    /// # Returns
    ///
    /// A cloned set of selected album IDs
    #[must_use]
    pub fn get_selected_album_ids(&self) -> HashSet<i64> {
        self.library.read().selected_album_ids.clone()
    }

    /// Checks if any albums are selected.
    ///
    /// # Returns
    ///
    /// `true` if any albums are selected, `false` otherwise
    #[must_use]
    pub fn has_selected_albums(&self) -> bool {
        !self.library.read().selected_album_ids.is_empty()
    }

    /// Checks if an album is selected.
    ///
    /// # Arguments
    ///
    /// * `album_id` - Album ID to check
    ///
    /// # Returns
    ///
    /// `true` if selected, `false` otherwise
    #[must_use]
    pub fn is_album_selected(&self, album_id: i64) -> bool {
        self.library.read().selected_album_ids.contains(&album_id)
    }

    /// Toggles select all/deselect all for the current library tab.
    ///
    /// If all items in the current tab are already selected, deselects all.
    /// Otherwise, selects all items in the current tab.
    ///
    /// This is context-aware: operates on albums if on Albums tab, artists if on Artists tab.
    pub fn toggle_select_all(&self) {
        let library = self.library.read();
        match library.current_tab {
            LibraryTab::Albums => {
                let all_selected = self.are_all_albums_selected();
                drop(library);
                if all_selected {
                    self.clear_album_selection();
                } else {
                    self.select_all_albums();
                }
            }
            LibraryTab::Artists => {
                let all_selected = self.are_all_artists_selected();
                drop(library);
                if all_selected {
                    self.clear_artist_selection();
                } else {
                    self.select_all_artists();
                }
            }
        }
    }

    /// Selects all albums.
    pub fn select_all_albums(&self) {
        let mut library = self.library.write();
        library.selected_album_ids = library.albums.iter().map(|a| a.id).collect();
        drop(library);
        self.broadcast_selection_change(LibraryTab::Albums);
    }

    /// Checks if all albums are selected.
    ///
    /// # Returns
    ///
    /// `true` if all albums are selected, `false` otherwise
    #[must_use]
    pub fn are_all_albums_selected(&self) -> bool {
        let library = self.library.read();
        !library.albums.is_empty() && library.selected_album_ids.len() == library.albums.len()
    }

    /// Updates the album selection with a new set of IDs.
    ///
    /// # Arguments
    ///
    /// * `selected_ids` - New set of selected album IDs
    pub fn update_album_selection(&self, selected_ids: HashSet<i64>) {
        let changed = {
            let mut library = self.library.write();
            if library.selected_album_ids == selected_ids {
                false
            } else {
                library.selected_album_ids = selected_ids;
                true
            }
        };

        if changed {
            self.broadcast_selection_change(LibraryTab::Albums);
        }
    }

    /// Toggles artist selection by ID.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - Artist ID to toggle
    ///
    /// # Returns
    ///
    /// `true` if the artist is now selected, `false` if deselected
    #[must_use]
    pub fn toggle_artist_selection(&self, artist_id: i64) -> bool {
        let mut library = self.library.write();
        if library.selected_artist_ids.contains(&artist_id) {
            library.selected_artist_ids.remove(&artist_id);
            false
        } else {
            library.selected_artist_ids.insert(artist_id);
            true
        }
    }

    /// Selects an artist by ID.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - Artist ID to select
    pub fn select_artist(&self, artist_id: i64) {
        self.library.write().selected_artist_ids.insert(artist_id);
        self.broadcast_selection_change(LibraryTab::Artists);
    }

    /// Deselects an artist by ID.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - Artist ID to deselect
    pub fn deselect_artist(&self, artist_id: i64) {
        self.library.write().selected_artist_ids.remove(&artist_id);
        self.broadcast_selection_change(LibraryTab::Artists);
    }

    /// Clears all artist selections.
    pub fn clear_artist_selection(&self) {
        self.library.write().selected_artist_ids.clear();
        self.broadcast_selection_change(LibraryTab::Artists);
    }

    /// Gets all selected artist IDs.
    ///
    /// # Returns
    ///
    /// A cloned set of selected artist IDs
    #[must_use]
    pub fn get_selected_artist_ids(&self) -> HashSet<i64> {
        self.library.read().selected_artist_ids.clone()
    }

    /// Checks if any artists are selected.
    ///
    /// # Returns
    ///
    /// `true` if any artists are selected, `false` otherwise
    #[must_use]
    pub fn has_selected_artists(&self) -> bool {
        !self.library.read().selected_artist_ids.is_empty()
    }

    /// Checks if an artist is selected.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - Artist ID to check
    ///
    /// # Returns
    ///
    /// `true` if selected, `false` otherwise
    #[must_use]
    pub fn is_artist_selected(&self, artist_id: i64) -> bool {
        self.library.read().selected_artist_ids.contains(&artist_id)
    }

    /// Selects all artists.
    pub fn select_all_artists(&self) {
        let mut library = self.library.write();
        library.selected_artist_ids = library.artists.iter().map(|a| a.id).collect();
        drop(library);
        self.broadcast_selection_change(LibraryTab::Artists);
    }

    /// Checks if all artists are selected.
    ///
    /// # Returns
    ///
    /// `true` if all artists are selected, `false` otherwise
    #[must_use]
    pub fn are_all_artists_selected(&self) -> bool {
        let library = self.library.read();
        !library.artists.is_empty() && library.selected_artist_ids.len() == library.artists.len()
    }

    /// Updates the artist selection with a new set of IDs.
    ///
    /// # Arguments
    ///
    /// * `selected_ids` - New set of selected artist IDs
    pub fn update_artist_selection(&self, selected_ids: HashSet<i64>) {
        let changed = {
            let mut library = self.library.write();
            if library.selected_artist_ids == selected_ids {
                false
            } else {
                library.selected_artist_ids = selected_ids;
                true
            }
        };

        if changed {
            self.broadcast_selection_change(LibraryTab::Artists);
        }
    }

    /// Updates the search filter and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `filter` - New search filter.
    pub fn update_search_filter(&self, filter: Option<String>) {
        debug!("AppState: Updating search filter to {:?}", filter);
        self.library.write().search_filter.clone_from(&filter);
        self.broadcast_event(&AppStateEvent::SearchFilterChanged(filter));
    }

    /// Clears the search filter without broadcasting an event.
    ///
    /// This is used when switching tabs to avoid updating views that are
    /// still visible during the crossfade transition.
    pub fn clear_search_filter_silent(&self) {
        debug!("AppState: Clearing search filter (silent)");
        self.library.write().search_filter = None;
    }

    /// Updates exclusive mode setting and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `enabled` - New exclusive mode state
    pub fn update_exclusive_mode(&self, enabled: bool) {
        debug!("AppState: Updating exclusive mode to {}", enabled);
        self.broadcast_event(&AppStateEvent::ExclusiveModeChanged { enabled });
    }

    /// Reports exclusive mode playback failure and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `reason` - Reason for the failure
    pub fn report_exclusive_mode_failure(&self, reason: String) {
        debug!("AppState: Reporting exclusive mode failure: {}", reason);
        self.broadcast_event(&AppStateEvent::ExclusiveModeFailed { reason });
    }

    /// Reports library scan failure and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `reason` - Reason for the failure
    pub fn report_library_scan_failure(&self, reason: String) {
        debug!("AppState: Reporting library scan failure: {}", reason);
        self.broadcast_event(&AppStateEvent::LibraryScanFailed { reason });
    }

    /// Sets the library scanning state and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `scanning` - Whether a scan is in progress
    pub fn set_scanning(&self, scanning: bool) {
        debug!("AppState: Setting scanning state to {}", scanning);
        self.is_scanning.store(scanning, Relaxed);
        self.broadcast_event(&AppStateEvent::LibraryScanningChanged {
            is_scanning: scanning,
        });
    }

    /// Gets the current scanning state.
    ///
    /// # Returns
    ///
    /// `true` if a scan is in progress, `false` otherwise
    #[must_use]
    pub fn is_scanning(&self) -> bool {
        self.is_scanning.load(Relaxed)
    }

    /// Broadcasts library scan progress.
    ///
    /// # Arguments
    ///
    /// * `current` - Number of albums processed so far
    /// * `total` - Total number of albums to process
    pub fn broadcast_scan_progress(&self, current: usize, total: usize) {
        self.broadcast_event(&AppStateEvent::LibraryScanProgress { current, total });
    }

    /// Subscribes to application state changes.
    ///
    /// # Returns
    ///
    /// A receiver for state change events.
    pub fn subscribe(&self) -> Receiver<Arc<AppStateEvent>> {
        debug!("AppState: New subscription created");

        // Create a new unbounded channel for this subscriber
        let (tx, rx) = unbounded();

        // Add sender to the list
        self.subscribers.write().push(tx);

        rx
    }

    /// Gets the current playback state.
    ///
    /// # Returns
    ///
    /// The current `PlaybackState`.
    #[must_use]
    pub fn get_playback_state(&self) -> PlaybackState {
        self.playback.read().clone()
    }

    /// Gets the current track information.
    ///
    /// # Returns
    ///
    /// The current `Option<TrackInfo>`.
    #[must_use]
    pub fn get_current_track(&self) -> Option<TrackInfo> {
        self.current_track.read().clone()
    }

    /// Gets the current playing album ID.
    ///
    /// # Returns
    ///
    /// The current playing album ID as `Option<i64>`.
    #[must_use]
    pub fn get_current_album_id(&self) -> Option<i64> {
        *self.current_album_id.read()
    }

    /// Updates the current playing album ID.
    ///
    /// # Arguments
    ///
    /// * `album_id` - The album ID to set
    pub fn update_current_album_id(&self, album_id: Option<i64>) {
        *self.current_album_id.write() = album_id;
    }

    /// Gets the current library state.
    ///
    /// # Returns
    ///
    /// The current `LibraryState`.
    #[must_use]
    pub fn get_library_state(&self) -> LibraryState {
        self.library.read().clone()
    }

    /// Gets the current navigation state.
    ///
    /// # Returns
    ///
    /// The current `NavigationState`.
    #[must_use]
    pub fn get_navigation_state(&self) -> NavigationState {
        self.navigation.read().clone()
    }

    /// Increases the grid view zoom level and persists to settings.
    pub fn increase_grid_zoom_level(&self) {
        let current_level = self.zoom_manager.get_grid_zoom_level();
        if current_level < 4 {
            // Update zoom manager (which handles persistence)
            self.zoom_manager.set_grid_zoom_level(current_level + 1);
        }
    }

    /// Decreases the grid view zoom level and persists to settings.
    pub fn decrease_grid_zoom_level(&self) {
        let current_level = self.zoom_manager.get_grid_zoom_level();
        if current_level > 0 {
            // Update zoom manager (which handles persistence)
            self.zoom_manager.set_grid_zoom_level(current_level - 1);
        }
    }

    /// Increases the list view zoom level and persists to settings.
    pub fn increase_list_zoom_level(&self) {
        let current_level = self.zoom_manager.get_list_zoom_level();
        if current_level < 2 {
            // Update zoom manager (which handles persistence)
            self.zoom_manager.set_list_zoom_level(current_level + 1);
        }
    }

    /// Decreases the list view zoom level and persists to settings.
    pub fn decrease_list_zoom_level(&self) {
        let current_level = self.zoom_manager.get_list_zoom_level();
        if current_level > 0 {
            // Update zoom manager (which handles persistence)
            self.zoom_manager.set_list_zoom_level(current_level - 1);
        }
    }

    /// Updates the `show_dr_values` setting and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `show_dr_values` - New value for the `show_dr_values` setting
    pub fn update_show_dr_values_setting(&self, show_dr_values: bool) {
        debug!(
            "AppState: Updating show_dr_values setting to {}",
            show_dr_values
        );

        // Update settings in settings manager
        if let Err(e) = self
            .settings_manager
            .write()
            .update_settings_with(|settings| {
                settings.show_dr_values = show_dr_values;
            })
        {
            debug!("Failed to update show_dr_values setting: {}", e);
            return;
        }

        // Broadcast settings change event
        self.broadcast_event(&AppStateEvent::SettingsChanged { show_dr_values });
    }

    /// Gets the settings manager reference.
    ///
    /// # Returns
    ///
    /// A reference to the settings manager.
    #[must_use]
    pub fn get_settings_manager(&self) -> Arc<RwLock<SettingsManager>> {
        Arc::clone(&self.settings_manager)
    }

    /// Updates the `show_metadata_overlays` setting and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `show_overlays` - New value for the `show_metadata_overlays` setting
    pub fn update_show_metadata_overlays_setting(&self, show_overlays: bool) {
        debug!(
            "AppState: Updating show_metadata_overlays setting to {}",
            show_overlays
        );

        // Update settings in settings manager
        if let Err(e) = self
            .settings_manager
            .write()
            .update_settings_with(|settings| {
                settings.show_metadata_overlays = show_overlays;
            })
        {
            debug!("Failed to update show_metadata_overlays setting: {}", e);
            return;
        }

        // Broadcast settings change event
        self.broadcast_event(&AppStateEvent::MetadataOverlaysChanged { show_overlays });
    }

    /// Updates the `year_display_mode` setting and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `mode` - New value for the `year_display_mode` setting ("release" or "original")
    pub fn update_year_display_mode_setting(&self, mode: String) {
        debug!("AppState: Updating year_display_mode setting to {}", mode);

        // Update settings in settings manager
        if let Err(e) = self
            .settings_manager
            .write()
            .update_settings_with(|settings| {
                settings.year_display_mode.clone_from(&mode);
            })
        {
            debug!("Failed to update year_display_mode setting: {}", e);
            return;
        }

        // Broadcast settings change event
        self.broadcast_event(&AppStateEvent::YearDisplayModeChanged { mode });
    }

    /// Updates the playback queue and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `queue` - New queue state
    pub fn update_queue(&self, queue: PlaybackQueue) {
        debug!(
            "AppState: Updating queue - {} tracks, current index: {:?}",
            queue.tracks.len(),
            queue.current_index
        );
        self.broadcast_event(&AppStateEvent::QueueChanged(queue));
    }
    /// Updates the grid sort configuration for albums and broadcasts the change.
    pub fn update_albums_grid_sort(&self, sort_items: Vec<AlbumGridSortItem>) {
        if let Err(e) = self
            .settings_manager
            .write()
            .update_settings_with(|s| s.albums_grid_sort = sort_items)
        {
            error!(error = %e, "Failed to update albums grid sort");
        }
        self.broadcast_event(&AppStateEvent::GridSortChanged(LibraryTab::Albums));
    }

    /// Updates the grid sort configuration for artists and broadcasts the change.
    pub fn update_artists_grid_sort(&self, sort_items: Vec<ArtistGridSortItem>) {
        if let Err(e) = self
            .settings_manager
            .write()
            .update_settings_with(|s| s.artists_grid_sort = sort_items)
        {
            error!(error = %e, "Failed to update artists grid sort");
        }
        self.broadcast_event(&AppStateEvent::GridSortChanged(LibraryTab::Artists));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail},
        parking_lot::RwLock,
    };

    use crate::{
        audio::engine::{AudioEngine, PlaybackState::Stopped},
        config::settings::SettingsManager,
        state::app_state::{
            AppState, LibraryState,
            LibraryTab::{Albums, Artists},
            ViewMode::{Grid, List},
        },
    };

    #[test]
    fn app_state_creation() -> Result<()> {
        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        if app_state.get_playback_state() != Stopped {
            bail!("Expected playback state to be Stopped");
        }
        if app_state.get_current_track().is_some() {
            bail!("Expected current track to be None");
        }
        if app_state.get_library_state().view_mode != Grid {
            bail!("Expected view mode to be Grid");
        }
        Ok(())
    }

    #[test]
    fn library_state_default() {
        let library_state = LibraryState::default();
        assert!(library_state.albums.is_empty());
        assert!(library_state.artists.is_empty());
        assert!(library_state.selected_album.is_none());
        assert!(library_state.selected_artist.is_none());
        assert!(library_state.current_tracks.is_empty());
        assert!(library_state.search_filter.is_none());
        assert_eq!(library_state.view_mode, Grid);
        assert_eq!(library_state.current_tab, Albums);
    }

    #[test]
    fn view_mode_display() {
        assert_eq!(format!("{Grid:?}"), "Grid");
        assert_eq!(format!("{List:?}"), "List");
    }

    #[test]
    fn library_tab_display() {
        assert_eq!(format!("{Albums:?}"), "Albums");
        assert_eq!(format!("{Artists:?}"), "Artists");
    }
}

//! Global application state with reactive update mechanisms.
//!
//! This module provides the central `AppState` container that manages
//! shared state across the application with thread-safe access and
//! reactive update notifications.

use std::sync::{Arc, Weak};

use {
    async_channel::{Receiver, Sender, unbounded},
    parking_lot::RwLock,
    tracing::debug,
};

use crate::{
    audio::engine::{
        AudioEngine,
        PlaybackState::{self, Stopped},
        TrackInfo,
    },
    config::SettingsManager,
    library::{Album, Artist, Track, scanner::LibraryScanner},
    state::zoom_manager::ZoomManager,
};

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
    /// Current library view state.
    pub library: Arc<RwLock<LibraryState>>,
    /// Current navigation state.
    pub navigation: Arc<RwLock<NavigationState>>,
    /// Audio engine reference.
    pub audio_engine: Weak<AudioEngine>,
    /// Library scanner reference (optional).
    pub library_scanner: Arc<RwLock<Option<Arc<RwLock<LibraryScanner>>>>>,
    /// List of active subscribers for manual broadcast fan-out.
    /// We use `async_channel` to avoid Tokio runtime dependencies in the waker logic.
    subscribers: Arc<RwLock<Vec<Sender<AppStateEvent>>>>,
    /// Zoom manager for handling view zoom levels.
    pub zoom_manager: Arc<ZoomManager>,
    /// Settings manager reference for persistence (wrapped in `RwLock` for mutability).
    pub settings_manager: Arc<RwLock<SettingsManager>>,
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
    /// User settings changed that affect UI display.
    SettingsChanged { show_dr_values: bool },
    /// Metadata overlays visibility setting changed.
    MetadataOverlaysChanged { show_overlays: bool },
    /// Year display mode setting changed.
    YearDisplayModeChanged { mode: String },
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
        library_scanner: Option<Arc<RwLock<LibraryScanner>>>,
        settings_manager: Arc<RwLock<SettingsManager>>,
    ) -> Self {
        let zoom_manager = Arc::new(ZoomManager::new(settings_manager.clone()));

        Self {
            playback: Arc::new(RwLock::new(Stopped)),
            current_track: Arc::new(RwLock::new(None)),
            library: Arc::new(RwLock::new(LibraryState::default())),
            navigation: Arc::new(RwLock::new(NavigationState::default())),
            audio_engine,
            library_scanner: Arc::new(RwLock::new(library_scanner)),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            zoom_manager,
            settings_manager,
        }
    }

    /// Helper to broadcast an event to all subscribers.
    /// Cleans up closed channels.
    fn broadcast_event(&self, event: AppStateEvent) -> usize {
        let mut subscribers = self.subscribers.write();
        let mut active = Vec::with_capacity(subscribers.len());
        let mut count = 0;

        for tx in subscribers.iter() {
            // We use try_send to avoid blocking. Since these are unbounded channels,
            // try_send should only fail if the channel is closed.
            // If it were bounded and full, this would return an error, effectively dropping the event.
            // But for UI events, unbounded is preferable to ensure delivery.
            if let Ok(()) = tx.try_send(event.clone()) {
                active.push(tx.clone());
                count += 1;
            }
        }

        *subscribers = active;
        count
    }

    /// Updates the playback state and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `state` - New playback state.
    pub fn update_playback_state(&self, state: PlaybackState) {
        debug!("AppState: Updating playback state to {:?}", state);
        *self.playback.write() = state.clone();
        self.broadcast_event(AppStateEvent::PlaybackStateChanged(state));
    }

    /// Updates the current track and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `track` - New current track information.
    pub fn update_current_track(&self, track: Option<TrackInfo>) {
        *self.current_track.write() = track.clone();
        self.broadcast_event(AppStateEvent::CurrentTrackChanged(Box::new(track)));
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
            library.albums = albums.clone();
            library.artists = artists.clone();
        }

        self.broadcast_event(AppStateEvent::LibraryDataChanged { albums, artists });
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
            self.broadcast_event(AppStateEvent::NavigationChanged(Box::new(state)));
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
            self.broadcast_event(AppStateEvent::ViewOptionsChanged {
                current_tab,
                view_mode,
            });
        }
    }

    /// Updates the search filter and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `filter` - New search filter.
    pub fn update_search_filter(&self, filter: Option<String>) {
        debug!("AppState: Updating search filter to {:?}", filter);
        self.library.write().search_filter = filter.clone();
        self.broadcast_event(AppStateEvent::SearchFilterChanged(filter));
    }

    /// Subscribes to application state changes.
    ///
    /// # Returns
    ///
    /// A receiver for state change events.
    pub fn subscribe(&self) -> Receiver<AppStateEvent> {
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
        let settings_write = self.settings_manager.write();
        let mut current_settings = settings_write.get_settings().clone();
        current_settings.show_dr_values = show_dr_values;

        if let Err(e) = settings_write.update_settings(current_settings) {
            debug!("Failed to update show_dr_values setting: {}", e);
            return;
        }
        drop(settings_write);

        // Broadcast settings change event
        self.broadcast_event(AppStateEvent::SettingsChanged { show_dr_values });
    }

    /// Gets the settings manager reference.
    ///
    /// # Returns
    ///
    /// A reference to the settings manager.
    #[must_use]
    pub fn get_settings_manager(&self) -> Arc<RwLock<SettingsManager>> {
        self.settings_manager.clone()
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
        let settings_write = self.settings_manager.write();
        let mut current_settings = settings_write.get_settings().clone();
        current_settings.show_metadata_overlays = show_overlays;

        if let Err(e) = settings_write.update_settings(current_settings) {
            debug!("Failed to update show_metadata_overlays setting: {}", e);
            return;
        }
        drop(settings_write);

        // Broadcast settings change event
        self.broadcast_event(AppStateEvent::MetadataOverlaysChanged { show_overlays });
    }

    /// Updates the `year_display_mode` setting and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `mode` - New value for the `year_display_mode` setting ("release" or "original")
    pub fn update_year_display_mode_setting(&self, mode: String) {
        debug!("AppState: Updating year_display_mode setting to {}", mode);

        // Update settings in settings manager
        let settings_write = self.settings_manager.write();
        let mut current_settings = settings_write.get_settings().clone();
        current_settings.year_display_mode = mode.clone();

        if let Err(e) = settings_write.update_settings(current_settings) {
            debug!("Failed to update year_display_mode setting: {}", e);
            return;
        }
        drop(settings_write);

        // Broadcast settings change event
        self.broadcast_event(AppStateEvent::YearDisplayModeChanged { mode });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::RwLock;

    use crate::{
        audio::engine::{AudioEngine, PlaybackState::Stopped},
        config::SettingsManager,
        state::{
            AppState, LibraryState,
            LibraryTab::{Albums, Artists},
            ViewMode::{Grid, List},
        },
    };

    #[test]
    fn test_app_state_creation() {
        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        assert_eq!(app_state.get_playback_state(), Stopped);
        assert!(app_state.get_current_track().is_none());
        assert_eq!(app_state.get_library_state().view_mode, Grid);
    }

    #[test]
    fn test_library_state_default() {
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
    fn test_view_mode_display() {
        assert_eq!(format!("{:?}", Grid), "Grid");
        assert_eq!(format!("{:?}", List), "List");
    }

    #[test]
    fn test_library_tab_display() {
        assert_eq!(format!("{:?}", Albums), "Albums");
        assert_eq!(format!("{:?}", Artists), "Artists");
    }
}

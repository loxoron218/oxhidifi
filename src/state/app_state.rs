//! Global application state with reactive update mechanisms.
//!
//! This module provides the central `AppState` container that manages
//! shared state across the application with thread-safe access and
//! reactive update notifications.

use std::sync::{Arc, Weak};

use {
    async_trait::async_trait,
    libadwaita::glib::MainContext,
    parking_lot::RwLock,
    tokio::sync::broadcast::{Receiver, Sender, channel},
};

use crate::{
    audio::engine::{
        AudioEngine,
        PlaybackState::{self, Stopped},
        TrackInfo,
    },
    library::{Album, Artist, Track, scanner::LibraryScanner},
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
    /// Audio engine reference.
    pub audio_engine: Weak<AudioEngine>,
    /// Library scanner reference (optional).
    pub library_scanner: Option<Arc<RwLock<LibraryScanner>>>,
    /// Broadcast channel for state change notifications.
    state_tx: Sender<AppStateEvent>,
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
    CurrentTrackChanged(Option<TrackInfo>),
    /// Library state changed.
    LibraryStateChanged(LibraryState),
    /// Search filter changed.
    SearchFilterChanged(Option<String>),
}

impl AppState {
    /// Creates a new application state instance.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Reference to the audio engine.
    /// * `library_scanner` - Optional library scanner reference.
    ///
    /// # Returns
    ///
    /// A new `AppState` instance.
    pub fn new(
        audio_engine: Weak<AudioEngine>,
        library_scanner: Option<Arc<RwLock<LibraryScanner>>>,
    ) -> Self {
        let (state_tx, _) = channel(16);

        Self {
            playback: Arc::new(RwLock::new(Stopped)),
            current_track: Arc::new(RwLock::new(None)),
            library: Arc::new(RwLock::new(LibraryState::default())),
            audio_engine,
            library_scanner,
            state_tx,
        }
    }

    /// Updates the playback state and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `state` - New playback state.
    pub fn update_playback_state(&self, state: PlaybackState) {
        *self.playback.write() = state.clone();
        let _ = self
            .state_tx
            .send(AppStateEvent::PlaybackStateChanged(state));
    }

    /// Updates the current track and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `track` - New current track information.
    pub fn update_current_track(&self, track: Option<TrackInfo>) {
        *self.current_track.write() = track.clone();
        let _ = self
            .state_tx
            .send(AppStateEvent::CurrentTrackChanged(track));
    }

    /// Updates the library state and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `library_state` - New library state.
    pub fn update_library_state(&self, library_state: LibraryState) {
        *self.library.write() = library_state.clone();
        let _ = self
            .state_tx
            .send(AppStateEvent::LibraryStateChanged(library_state));
    }

    /// Updates the search filter and notifies subscribers.
    ///
    /// # Arguments
    ///
    /// * `filter` - New search filter.
    pub fn update_search_filter(&self, filter: Option<String>) {
        self.library.write().search_filter = filter.clone();
        let _ = self
            .state_tx
            .send(AppStateEvent::SearchFilterChanged(filter));
    }

    /// Subscribes to application state changes.
    ///
    /// # Returns
    ///
    /// A broadcast receiver for state change events.
    pub fn subscribe(&self) -> Receiver<AppStateEvent> {
        self.state_tx.subscribe()
    }

    /// Gets the current playback state.
    ///
    /// # Returns
    ///
    /// The current `PlaybackState`.
    pub fn get_playback_state(&self) -> PlaybackState {
        self.playback.read().clone()
    }

    /// Gets the current track information.
    ///
    /// # Returns
    ///
    /// The current `Option<TrackInfo>`.
    pub fn get_current_track(&self) -> Option<TrackInfo> {
        self.current_track.read().clone()
    }

    /// Gets the current library state.
    ///
    /// # Returns
    ///
    /// The current `LibraryState`.
    pub fn get_library_state(&self) -> LibraryState {
        self.library.read().clone()
    }
}

/// Trait for UI components to subscribe to state changes.
///
/// This trait allows UI components to react to application state changes
/// without tight coupling to the state management system.
#[async_trait(?Send)]
pub trait StateObserver {
    /// Handles a state change event.
    ///
    /// # Arguments
    ///
    /// * `event` - The state change event to handle.
    async fn handle_state_change(&mut self, event: AppStateEvent);

    /// Starts observing state changes.
    ///
    /// # Arguments
    ///
    /// * `app_state` - The application state to observe.
    async fn start_observing(&mut self, app_state: Arc<AppState>) {
        let mut receiver = app_state.subscribe();

        // Use glib::MainContext for GTK thread safety
        MainContext::default().spawn_local(async move {
            while let Ok(_event) = receiver.recv().await {
                // Note: We can't call self.handle_state_change directly here
                // because of ownership issues. Instead, UI components should
                // subscribe to state changes directly and handle them appropriately.
                // This is a limitation of the current architecture that would be
                // addressed in a more sophisticated implementation.
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        audio::engine::{AudioEngine, PlaybackState::Stopped},
        state::{
            AppState,
            LibraryState::{Albums, Artists},
            ViewMode::{Grid, List},
        },
    };

    #[test]
    fn test_app_state_creation() {
        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let app_state = AppState::new(engine_weak);

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

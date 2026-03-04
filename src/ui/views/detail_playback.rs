//! Playback handling logic for detail views.

use std::sync::Arc;

use {
    libadwaita::{Toast, ToastOverlay, glib::MainContext},
    thiserror::Error,
    tracing::{error, warn},
};

use crate::{
    audio::{
        constants::{DEFAULT_BIT_DEPTH, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE},
        decoder_types::AudioFormat,
        engine::{
            AudioEngine, AudioError as EngineAudioError,
            PlaybackState::{Paused, Playing},
            TrackInfo,
        },
        metadata::{MetadataError, TagReader},
        queue_manager::QueueManager,
    },
    error::audio_reporting::handle_exclusive_mode_error,
    library::{
        database::{LibraryDatabase, LibraryError},
        models::Track,
    },
    state::app_state::AppState,
    ui::views::detail_types::TrackTechDetails,
};

/// Error type for track playback operations.
#[derive(Debug, Error)]
pub enum PlaybackError {
    /// No tracks found in album.
    #[error("No tracks found in album {0}")]
    NoTracks(i64),
    /// Failed to load track.
    #[error("Load error: {0}")]
    LoadError(#[source] EngineAudioError),
    /// Failed to play track.
    #[error("Play error: {0}")]
    PlayError(#[source] EngineAudioError),
    /// Database error.
    #[error("Database error: {0}")]
    DatabaseError(#[source] LibraryError),
    /// Metadata read error.
    #[error("Metadata error: {0}")]
    MetadataError(#[source] MetadataError),
    /// Invalid audio format.
    #[error("Invalid format for {path}: {field}")]
    InvalidFormat { path: String, field: String },
    /// Audio engine missing.
    #[error("Audio engine not available")]
    AudioEngineMissing,
    /// Queue manager missing.
    #[error("Queue manager not available")]
    QueueManagerMissing,
}

/// Handler for track playback operations.
#[derive(Clone)]
pub struct PlaybackHandler {
    /// Audio engine reference for playback.
    audio_engine: Option<Arc<AudioEngine>>,
    /// Queue manager reference for queue operations.
    queue_manager: Option<Arc<QueueManager>>,
}

impl PlaybackHandler {
    /// Creates a new playback handler.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Audio engine reference for playback
    /// * `queue_manager` - Queue manager reference for queue operations
    ///
    /// # Returns
    ///
    /// A new `PlaybackHandler` instance.
    #[must_use]
    pub fn new(
        audio_engine: Option<Arc<AudioEngine>>,
        queue_manager: Option<Arc<QueueManager>>,
    ) -> Self {
        Self {
            audio_engine,
            queue_manager,
        }
    }

    /// Creates a track click handler callback.
    ///
    /// # Arguments
    ///
    /// * `album_tracks` - Pre-loaded album tracks for queue setup
    /// * `toast_overlay` - Toast overlay for displaying feedback messages
    ///
    /// # Returns
    ///
    /// A callback function for handling track clicks
    pub fn create_track_click_handler(
        &self,
        album_tracks: Vec<Track>,
        toast_overlay: ToastOverlay,
    ) -> impl Fn(Track) + Clone + 'static {
        let audio_engine = self.audio_engine.clone();
        let queue_manager = self.queue_manager.clone();

        move |track: Track| {
            if audio_engine.is_none() || queue_manager.is_none() {
                warn!("Missing dependencies for track playback");
                let missing_deps = if audio_engine.is_none() && queue_manager.is_none() {
                    "audio engine and queue manager"
                } else if audio_engine.is_none() {
                    "audio engine"
                } else {
                    "queue manager"
                };
                let toast = Toast::new(&format!("Playback not available - missing {missing_deps}"));
                toast_overlay.add_toast(toast);
                return;
            }

            let tech_details = TrackTechDetails::from_track(&track);
            let audio_engine_clone = audio_engine.clone();
            let queue_manager_clone = queue_manager.clone();
            let album_tracks_clone = album_tracks.clone();
            let toast_overlay = toast_overlay.clone();
            let track_path = track.path;

            MainContext::default().spawn_local(async move {
                match Self::load_and_play_track(
                    &track_path,
                    tech_details,
                    album_tracks_clone,
                    audio_engine_clone,
                    queue_manager_clone,
                )
                .await
                {
                    Ok(_) => {}
                    Err(e) => Self::show_playback_error(&toast_overlay, &e, &track_path),
                }
            });
        }
    }

    /// Loads album tracks and initiates playback of a specific track.
    ///
    /// # Arguments
    ///
    /// * `track_path` - Path to the track file
    /// * `tech_details` - Track technical details
    /// * `album_tracks` - Pre-loaded album tracks for queue setup
    /// * `audio_engine` - Audio engine reference
    /// * `queue_manager` - Queue manager reference
    ///
    /// # Returns
    ///
    /// A `Result` containing the `TrackInfo` or a `PlaybackError`
    ///
    /// # Errors
    ///
    /// Returns `PlaybackError::QueueManagerMissing` if the queue manager is not available.
    /// Returns `PlaybackError::AudioEngineMissing` if the audio engine is not available.
    /// Returns `PlaybackError::LoadError` if the track file cannot be loaded.
    /// Returns `PlaybackError::PlayError` if playback cannot be started.
    /// Returns `PlaybackError::MetadataError` if track metadata cannot be read.
    /// Returns `PlaybackError::InvalidFormat` if technical details contain invalid values.
    pub async fn load_and_play_track(
        track_path: &str,
        tech_details: TrackTechDetails,
        album_tracks: Vec<Track>,
        audio_engine: Option<Arc<AudioEngine>>,
        queue_manager: Option<Arc<QueueManager>>,
    ) -> Result<Option<TrackInfo>, PlaybackError> {
        Self::set_playback_queue(queue_manager, album_tracks)?;
        Self::load_and_start_playback(audio_engine, track_path).await?;
        Self::read_and_create_track_info(track_path, tech_details).map(Some)
    }

    /// Sets the playback queue with album tracks.
    ///
    /// # Arguments
    ///
    /// * `queue_manager` - Queue manager reference
    /// * `album_tracks` - Vector of album tracks
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure
    ///
    /// # Errors
    ///
    /// Returns `PlaybackError::QueueManagerMissing` if the queue manager is not available.
    fn set_playback_queue(
        queue_manager: Option<Arc<QueueManager>>,
        album_tracks: Vec<Track>,
    ) -> Result<(), PlaybackError> {
        queue_manager
            .ok_or(PlaybackError::QueueManagerMissing)
            .map(|qm| qm.set_queue(album_tracks))
    }

    /// Loads a track into the audio engine and starts playback.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Audio engine reference
    /// * `track_path` - Path to the track file
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure
    ///
    /// # Errors
    ///
    /// Returns `PlaybackError::AudioEngineMissing` if the audio engine is not available.
    /// Returns `PlaybackError::LoadError` if the track file cannot be loaded.
    /// Returns `PlaybackError::PlayError` if playback cannot be started.
    async fn load_and_start_playback(
        audio_engine: Option<Arc<AudioEngine>>,
        track_path: &str,
    ) -> Result<(), PlaybackError> {
        let engine = audio_engine.ok_or(PlaybackError::AudioEngineMissing)?;
        engine
            .load_track(track_path)
            .map_err(PlaybackError::LoadError)?;
        engine.play().await.map_err(PlaybackError::PlayError)?;
        Ok(())
    }

    /// Reads track metadata and creates a `TrackInfo` struct.
    ///
    /// # Arguments
    ///
    /// * `track_path` - Path to the track file
    /// * `tech_details` - Track technical details
    ///
    /// # Returns
    ///
    /// A `Result` containing the `TrackInfo` or a `PlaybackError`
    ///
    /// # Errors
    ///
    /// Returns an error if metadata reading fails or technical details are invalid.
    fn read_and_create_track_info(
        track_path: &str,
        tech_details: TrackTechDetails,
    ) -> Result<TrackInfo, PlaybackError> {
        let metadata =
            TagReader::read_metadata(track_path).map_err(PlaybackError::MetadataError)?;

        let sr = u32::try_from(tech_details.sample_rate).map_err(|_err| {
            PlaybackError::InvalidFormat {
                path: track_path.to_string(),
                field: "sample_rate".to_string(),
            }
        })?;
        let ch =
            u32::try_from(tech_details.channels).map_err(|_err| PlaybackError::InvalidFormat {
                path: track_path.to_string(),
                field: "channels".to_string(),
            })?;
        let bps = u32::try_from(tech_details.bits_per_sample).map_err(|_err| {
            PlaybackError::InvalidFormat {
                path: track_path.to_string(),
                field: "bits_per_sample".to_string(),
            }
        })?;
        let dur = u64::try_from(tech_details.duration_ms).map_err(|_err| {
            PlaybackError::InvalidFormat {
                path: track_path.to_string(),
                field: "duration_ms".to_string(),
            }
        })?;

        Ok(TrackInfo {
            path: track_path.to_string(),
            metadata,
            format: AudioFormat {
                sample_rate: sr,
                channels: ch,
                bits_per_sample: bps,
                channel_mask: 0,
            },
            duration_ms: dur,
        })
    }

    /// Displays an error toast based on the playback error type.
    ///
    /// # Arguments
    ///
    /// * `toast_overlay` - Toast overlay reference
    /// * `error` - The playback error that occurred
    /// * `track_path` - Path to the track for error logging
    fn show_playback_error(toast_overlay: &ToastOverlay, error: &PlaybackError, track_path: &str) {
        error!("Playback error for track {}: {}", track_path, error);

        let toast_message = match error {
            PlaybackError::NoTracks(_) => format!("No tracks found in album: {track_path}"),
            PlaybackError::LoadError(_) => format!("Failed to load track: {track_path}"),
            PlaybackError::PlayError(_) => format!("Failed to play track: {track_path}"),
            PlaybackError::DatabaseError(_) => {
                format!("Failed to load album tracks: {track_path}")
            }
            PlaybackError::MetadataError(_) => {
                format!("Failed to read track metadata: {track_path}")
            }
            PlaybackError::InvalidFormat { .. } => {
                format!("Unsupported audio format: {track_path}")
            }
            PlaybackError::AudioEngineMissing | PlaybackError::QueueManagerMissing => return,
        };

        toast_overlay.add_toast(Toast::new(&toast_message));
    }
}

/// Plays an album, handling pause/resume for the current album or playing a new album.
///
/// # Arguments
///
/// * `album_id` - Album ID to play
/// * `library_db` - Library database reference
/// * `audio_engine` - Audio engine reference
/// * `queue_manager` - Queue manager reference
/// * `app_state` - Application state reference
pub async fn play_album(
    album_id: i64,
    library_db: Option<Arc<LibraryDatabase>>,
    audio_engine: Option<Arc<AudioEngine>>,
    queue_manager: Option<Arc<QueueManager>>,
    app_state: Option<Arc<AppState>>,
) {
    if let (Some(db), Some(qm), Some(engine), Some(state)) =
        (library_db, queue_manager, audio_engine, app_state)
    {
        let tracks = match db.get_tracks_by_album(album_id).await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to fetch tracks for album {}: {}", album_id, e);
                return;
            }
        };

        if tracks.is_empty() {
            warn!("No tracks found for album {}", album_id);
            return;
        }

        let current_album_id = state.get_current_album_id();
        let is_current_album = current_album_id == Some(album_id);
        let is_playing = state.get_playback_state() == Playing;

        if is_current_album && is_playing {
            if let Err(e) = engine.pause().await {
                if !handle_exclusive_mode_error(&e, &state) {
                    error!(error = %e, "Failed to pause playback");
                }
            } else {
                state.update_playback_state(Paused);
            }
            return;
        }

        if is_current_album && !is_playing {
            if let Err(e) = engine.resume().await {
                if !handle_exclusive_mode_error(&e, &state) {
                    error!(error = %e, "Failed to resume playback");
                }
            } else {
                state.update_playback_state(Playing);
            }
            return;
        }

        qm.set_queue(tracks.clone());

        let first_track = &tracks[0];
        let track_path = &first_track.path;

        if let Err(e) = engine.load_track(track_path) {
            error!(error = %e, "Failed to load track: {}", track_path);
            return;
        }

        if let Err(e) = engine.play().await {
            if !handle_exclusive_mode_error(&e, &state) {
                error!("Failed to start playback: {}", e);
            }
            return;
        }

        state.update_playback_state(Playing);
        state.update_current_album_id(Some(album_id));

        if let Ok(metadata) = TagReader::read_metadata(track_path) {
            let track_info = TrackInfo {
                path: track_path.clone(),
                metadata,
                format: AudioFormat {
                    sample_rate: u32::try_from(first_track.sample_rate)
                        .unwrap_or(DEFAULT_SAMPLE_RATE),
                    channels: u32::try_from(first_track.channels).unwrap_or(DEFAULT_CHANNELS),
                    bits_per_sample: u32::try_from(first_track.bits_per_sample)
                        .unwrap_or(DEFAULT_BIT_DEPTH),
                    channel_mask: 0,
                },
                duration_ms: u64::try_from(first_track.duration_ms).unwrap_or(0),
            };
            state.update_current_track(Some(track_info));
        }
    }
}

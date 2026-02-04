//! Type definitions and builder for detail view components.

use std::sync::Arc;

use thiserror::Error;

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    library::{
        database::LibraryDatabase,
        models::{Album, Artist, Track},
    },
    state::AppState,
    ui::views::detail_view::DetailView,
};

/// Type of detail to display.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailType {
    /// Display album detail
    Album(Album),
    /// Display artist detail
    Artist(Artist),
}

/// Error type for `DetailView` builder operations.
#[derive(Debug, Error)]
pub enum DetailViewBuildError {
    /// Missing required detail type.
    #[error("Missing required detail type")]
    MissingDetailType,
}

/// Configuration for `DetailView` display options.
#[derive(Debug, Clone)]
pub struct DetailViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

/// Builder pattern for configuring `DetailView` components.
#[derive(Default)]
pub struct DetailViewBuilder {
    /// Optional application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Optional library database reference.
    pub library_db: Option<Arc<LibraryDatabase>>,
    /// Optional audio engine reference.
    pub audio_engine: Option<Arc<AudioEngine>>,
    /// Optional queue manager reference.
    pub queue_manager: Option<Arc<QueueManager>>,
    /// The type of detail to display (album or artist).
    pub detail_type: Option<DetailType>,
    /// Whether to use compact layout.
    pub compact: bool,
}

/// Track technical details for playback.
#[derive(Debug, Clone, Copy)]
pub struct TrackTechDetails {
    /// Track sample rate.
    pub sample_rate: i64,
    /// Number of audio channels.
    pub channels: i64,
    /// Bits per sample.
    pub bits_per_sample: i64,
    /// Track duration in milliseconds.
    pub duration_ms: i64,
}

/// Result type for `DetailView` builder operations.
pub type BuildResult<T> = Result<T, DetailViewBuildError>;

impl DetailViewBuilder {
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

    /// Sets the queue manager for queue operations.
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

    /// Sets the detail type (album or artist).
    ///
    /// # Arguments
    ///
    /// * `detail_type` - The type of detail to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn detail_type(mut self, detail_type: Option<DetailType>) -> Self {
        self.detail_type = detail_type;
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

    /// Builds the `DetailView` component.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `DetailView` instance or a `DetailViewBuildError`.
    ///
    /// # Errors
    ///
    /// Returns an error if the required detail type is missing.
    pub fn build(self) -> Result<DetailView, DetailViewBuildError> {
        let detail_type = self
            .detail_type
            .ok_or(DetailViewBuildError::MissingDetailType)?;
        Ok(DetailView::new(
            self.app_state,
            self.library_db,
            self.audio_engine,
            self.queue_manager,
            detail_type,
            self.compact,
        ))
    }
}

impl TrackTechDetails {
    /// Creates track technical details from a `Track` model.
    ///
    /// # Arguments
    ///
    /// * `track` - Track model to extract technical details from
    ///
    /// # Returns
    ///
    /// A new `TrackTechDetails` instance with values from the track.
    #[must_use]
    pub fn from_track(track: &Track) -> Self {
        Self {
            sample_rate: track.sample_rate,
            channels: track.channels,
            bits_per_sample: track.bits_per_sample,
            duration_ms: track.duration_ms,
        }
    }
}

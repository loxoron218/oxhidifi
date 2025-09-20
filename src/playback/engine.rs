use std::{path::Path, sync::mpsc::Sender};

use super::{
    error::PlaybackError,
    events::{
        PlaybackEvent::{self, Error, PositionChanged, StateChanged},
        PlaybackState::{self, Paused, Playing, Stopped},
    },
    pipeline::PipelineManager,
};

/// Type alias for the playback event sender
///
/// This alias simplifies the type signature for sending playback events
/// from the engine to the UI components.
pub type PlaybackEventSender = Sender<PlaybackEvent>;

/// The core playback engine that manages audio playback
///
/// The `PlaybackEngine` is responsible for controlling audio playback operations
/// by managing a GStreamer pipeline through the [`PipelineManager`]. It maintains
/// the current playback state and communicates state changes and other events
/// to the UI via a channel.
///
/// # Fields
///
/// * `pipeline_manager` - Manages the underlying GStreamer pipeline for audio operations
/// * `event_sender` - Channel sender for notifying UI components of playback events
/// * `current_state` - The current playback state (Stopped, Playing, Paused)
pub struct PlaybackEngine {
    pipeline_manager: PipelineManager,
    event_sender: PlaybackEventSender,
    pub current_state: PlaybackState,
}

impl PlaybackEngine {
    /// Creates a new playback engine
    ///
    /// Initializes a new [`PlaybackEngine`] instance with a new [`PipelineManager`]
    /// and sets up the communication channel for event handling.
    ///
    /// # Parameters
    ///
    /// * `event_sender` - A channel sender for transmitting playback events to the UI
    ///
    /// # Returns
    ///
    /// Returns `Ok(PlaybackEngine)` if initialization is successful, or
    /// a [`PlaybackError`] if initialization fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to initialize.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// let (sender, receiver) = channel();
    /// let engine = PlaybackEngine::new(sender)
    ///     .expect("Failed to create playback engine");
    /// ```
    pub fn new(event_sender: PlaybackEventSender) -> Result<Self, PlaybackError> {
        let pipeline_manager = PipelineManager::new()?;
        Ok(Self {
            pipeline_manager,
            event_sender,
            current_state: Stopped,
        })
    }

    /// Loads a track for playback
    ///
    /// Prepares the specified audio file for playback by setting it as the source
    /// in the GStreamer pipeline. This operation does not start playback automatically;
    /// use [`play`](Self::play) to begin playback after loading.
    ///
    /// # Parameters
    ///
    /// * `path` - A reference to the file path of the audio track to load
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the track was successfully loaded, or a [`PlaybackError`]
    /// if loading failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to set the URI.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::{path::Path, sync::mpsc::channel};
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let mut engine = PlaybackEngine::new(sender).unwrap();
    /// let path = Path::new("/path/to/audio.mp3");
    /// engine.load_track(path)
    ///     .expect("Failed to load track");
    /// ```
    pub fn load_track(&mut self, path: &Path) -> Result<(), PlaybackError> {
        let uri = format!("file://{}", path.display());
        self.pipeline_manager.set_uri(&uri)?;
        self.current_state = Stopped;

        // Send state change event
        let _ = self.event_sender.send(StateChanged(Stopped));
        Ok(())
    }

    /// Starts playback
    ///
    /// Initiates playback of the currently loaded track by setting the pipeline
    /// state to Playing. If no track is loaded, this method will have no effect.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully started, or a [`PlaybackError`]
    /// if starting playback fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to start playback.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::{path::Path, sync::mpsc::channel};
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let mut engine = PlaybackEngine::new(sender).unwrap();
    /// engine.play()
    ///     .expect("Failed to start playback");
    /// ```
    pub fn play(&mut self) -> Result<(), PlaybackError> {
        match self.pipeline_manager.play() {
            Ok(_) => {
                self.current_state = Playing;
                let _ = self.event_sender.send(StateChanged(Playing));
                Ok(())
            }

            // Handle playback start error by logging and sending error event to UI
            Err(e) => {
                println!("Error starting playback: {:?}", e);

                // Send error event to UI
                let _ = self
                    .event_sender
                    .send(Error(format!("Playback error: {:?}", e)));
                Err(e)
            }
        }
    }

    /// Pauses playback
    ///
    /// Temporarily pauses playback while maintaining the current position.
    /// Playback can be resumed from the same position using [`play`](Self::play).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully paused, or a [`PlaybackError`]
    /// if pausing playback fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to pause playback.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let mut engine = PlaybackEngine::new(sender).unwrap();
    /// engine.pause()
    ///     .expect("Failed to pause playback");
    /// ```
    pub fn pause(&mut self) -> Result<(), PlaybackError> {
        self.pipeline_manager.pause()?;
        self.current_state = Paused;
        let _ = self.event_sender.send(StateChanged(Paused));
        Ok(())
    }

    /// Stops playback
    ///
    /// Stops playback and resets the position to the beginning of the track.
    /// To resume playback, the track must be reloaded with [`load_track`](Self::load_track).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully stopped, or a [`PlaybackError`]
    /// if stopping playback fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to stop playback.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let mut engine = PlaybackEngine::new(sender).unwrap();
    /// engine.stop()
    ///     .expect("Failed to stop playback");
    /// ```
    pub fn stop(&mut self) -> Result<(), PlaybackError> {
        self.pipeline_manager.stop()?;
        self.current_state = Stopped;
        let _ = self.event_sender.send(StateChanged(Stopped));
        Ok(())
    }

    /// Seeks to a specific position
    ///
    /// Changes the playback position to the specified time in nanoseconds.
    /// This operation can be performed during playback or when paused.
    ///
    /// # Parameters
    ///
    /// * `position_ns` - The target position in nanoseconds
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if seeking was successful, or a [`PlaybackError`]
    /// if seeking fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to seek.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let mut engine = PlaybackEngine::new(sender).unwrap();
    /// // Seek to 30 seconds (30,000,000 nanoseconds)
    /// engine.seek(30_000_000_000)
    ///     .expect("Failed to seek");
    /// ```
    pub fn seek(&mut self, position_ns: u64) -> Result<(), PlaybackError> {
        self.pipeline_manager.seek(position_ns)?;
        let _ = self.event_sender.send(PositionChanged(position_ns));
        Ok(())
    }

    /// Gets the current playback state
    ///
    /// Returns a reference to the current playback state of the engine.
    ///
    /// # Returns
    ///
    /// A reference to the current [`PlaybackState`].
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let engine = PlaybackEngine::new(sender).unwrap();
    /// let state = engine.get_current_state();
    /// ```
    pub fn get_current_state(&self) -> &PlaybackState {
        &self.current_state
    }

    /// Gets the duration of the current track
    ///
    /// Queries the pipeline for the total duration of the currently loaded track.
    /// The duration is returned in nanoseconds.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(u64))` with the duration in nanoseconds if available,
    /// `Ok(None)` if the duration is not available, or a [`PlaybackError`] if
    /// querying the duration fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to query the duration.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let engine = PlaybackEngine::new(sender).unwrap();
    /// let duration = engine.get_duration()
    ///     .expect("Failed to get duration");
    /// ```
    pub fn get_duration(&self) -> Result<Option<u64>, PlaybackError> {
        self.pipeline_manager.get_duration()
    }

    /// Gets the current position of playback
    ///
    /// Queries the pipeline for the current playback position in nanoseconds.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(u64))` with the current position in nanoseconds if available,
    /// `Ok(None)` if the position is not available, or a [`PlaybackError`] if
    /// querying the position fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`PipelineManager`] fails to query the position.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::mpsc::channel;
    /// # use crate::playback::engine::PlaybackEngine;
    /// # let (sender, receiver) = channel();
    /// # let engine = PlaybackEngine::new(sender).unwrap();
    /// let position = engine.get_position()
    ///     .expect("Failed to get position");
    /// ```
    pub fn get_position(&self) -> Result<Option<u64>, PlaybackError> {
        self.pipeline_manager.get_position()
    }
}

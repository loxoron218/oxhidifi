use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel},
    },
};

use crate::ui::components::player_bar::PlayerBar;

use super::{
    engine::PlaybackEngine,
    error::PlaybackError::{self, FileNotFound},
    events::{
        PlaybackEvent::{self, EndOfStream, Error, PositionChanged, StateChanged},
        PlaybackState,
    },
};

/// Controls playback and handles communication between the UI and playback engine.
///
/// The `PlaybackController` serves as the main interface for controlling audio playback
/// in the application. It manages the playback state, coordinates with the UI through
/// events, and delegates actual playback operations to the [`PlaybackEngine`].
///
/// # Fields
///
/// * `engine` - The underlying playback engine that handles actual audio operations
/// * `event_receiver` - Receives playback events from the engine
/// * `current_track` - Path to the currently loaded track, if any
/// * `duration` - Duration of the current track in nanoseconds, if available
/// * `position` - Current playback position in nanoseconds
/// * `player_bar` - Optional reference to the UI player bar component
pub struct PlaybackController {
    /// The playback engine responsible for actual audio operations
    engine: PlaybackEngine,
    /// Receiver for playback events from the engine
    event_receiver: Receiver<PlaybackEvent>,
    /// Path to the currently loaded track, if any
    current_track: Option<PathBuf>,
    /// Duration of the current track in nanoseconds, if available
    duration: Option<u64>,
    /// Current playback position in nanoseconds
    position: u64,
    /// Optional reference to the UI player bar component for event forwarding
    player_bar: Option<Arc<Mutex<PlayerBar>>>,
}

impl PlaybackController {
    /// Creates a new playback controller without a player bar.
    ///
    /// Initializes a new [`PlaybackController`] instance with a new [`PlaybackEngine`]
    /// and sets up the communication channels for event handling.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a tuple with:
    /// * The new `PlaybackController` instance
    /// * A `Sender<PlaybackEvent>` for sending events to the controller
    ///
    /// # Errors
    ///
    /// Returns a [`PlaybackError`] if the playback engine fails to initialize.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crate::playback::controller::PlaybackController;
    /// let (controller, event_sender) = PlaybackController::new()
    ///     .expect("Failed to create playback controller");
    /// ```
    pub fn new() -> Result<(Self, Sender<PlaybackEvent>), PlaybackError> {
        println!("Creating new playback controller");
        let (event_sender, event_receiver) = channel();
        let engine = PlaybackEngine::new(event_sender.clone())?;
        let controller = Self {
            engine,
            event_receiver,
            current_track: None,
            duration: None,
            position: 0,
            player_bar: None,
        };
        Ok((controller, event_sender))
    }

    /// Creates a new playback controller with a player bar.
    ///
    /// Initializes a new [`PlaybackController`] instance with a reference to a
    /// [`PlayerBar`] component for UI integration.
    ///
    /// # Parameters
    ///
    /// * `player_bar` - An `Arc<Mutex<PlayerBar>>` reference to the UI player bar
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a tuple with:
    /// * The new `PlaybackController` instance
    /// * A `Sender<PlaybackEvent>` for sending events to the controller
    ///
    /// # Errors
    ///
    /// Returns a [`PlaybackError`] if the playback engine fails to initialize.
    pub fn new_with_player_bar(
        player_bar: Arc<Mutex<PlayerBar>>,
    ) -> Result<(Self, Sender<PlaybackEvent>), PlaybackError> {
        let (event_sender, event_receiver) = channel();
        let engine = PlaybackEngine::new(event_sender.clone())?;
        let controller = Self {
            engine,
            event_receiver,
            current_track: None,
            duration: None,
            position: 0,
            player_bar: Some(player_bar),
        };
        Ok((controller, event_sender))
    }

    /// Loads a track for playback.
    ///
    /// Prepares the specified audio file for playback by loading it into the
    /// playback engine and querying its duration. The track is not automatically
    /// played; use [`play`](Self::play) to start playback.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to the audio file to load
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the track was successfully loaded, or a [`PlaybackError`]
    /// if loading failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * The file at `path` does not exist ([`FileNotFound`](PlaybackError::FileNotFound))
    /// * The playback engine fails to load the track
    /// * The duration query fails
    pub fn load_track(&mut self, path: PathBuf) -> Result<(), PlaybackError> {
        // Check if the file exists before trying to load it
        if !path.exists() {
            println!("Playback controller: File not found: {:?}", path);
            return Err(FileNotFound(path.clone()));
        }
        self.engine.load_track(&path)?;
        self.current_track = Some(path.clone());

        // Query the duration from GStreamer
        self.duration = self.engine.get_duration()?;
        Ok(())
    }

    /// Starts playback of the currently loaded track.
    ///
    /// Initiates playback of the track that was previously loaded with
    /// [`load_track`](Self::load_track). If no track is loaded, this method
    /// will have no effect.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully started, or a [`PlaybackError`]
    /// if starting playback failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the playback engine fails to start playback.
    pub fn play(&mut self) -> Result<(), PlaybackError> {
        let result = self.engine.play();
        result
    }

    /// Pauses playback of the currently playing track.
    ///
    /// Temporarily pauses playback, maintaining the current position.
    /// Playback can be resumed from the same position using [`play`](Self::play).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully paused, or a [`PlaybackError`]
    /// if pausing playback failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the playback engine fails to pause playback.
    pub fn pause(&mut self) -> Result<(), PlaybackError> {
        self.engine.pause()
    }

    /// Stops playback and resets the playback position.
    ///
    /// Stops playback and resets the position to the beginning of the track.
    /// To resume playback, the track must be reloaded with [`load_track`](Self::load_track)
    /// or playback must be restarted with [`play`](Self::play).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully stopped, or a [`PlaybackError`]
    /// if stopping playback failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the playback engine fails to stop playback.
    pub fn stop(&mut self) -> Result<(), PlaybackError> {
        self.engine.stop()
    }

    /// Seeks to a specific position in the currently loaded track.
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
    /// if seeking failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the playback engine fails to seek.
    pub fn seek(&mut self, position_ns: u64) -> Result<(), PlaybackError> {
        self.engine.seek(position_ns)
    }

    /// Handles incoming playback events from the engine.
    ///
    /// Processes all pending events from the playback engine, updating internal
    /// state and forwarding events to the UI player bar if one is connected.
    /// This method should be called regularly to ensure events are processed
    /// in a timely manner.
    ///
    /// Events handled include:
    /// * State changes (playing, paused, stopped)
    /// * Position updates
    /// * End of stream notifications
    /// * Error notifications
    pub fn handle_events(&mut self) {
        while let Ok(event) = self.event_receiver.try_recv() {
            // Send the event to the player bar if it exists
            if let Some(player_bar) = &self.player_bar {
                match player_bar.lock() {
                    Ok(player_bar) => {
                        player_bar.handle_playback_event(event.clone());
                    }
                    Err(e) => {
                        eprintln!("Failed to acquire lock on player bar: {}", e);
                    }
                }
            }

            match event {
                StateChanged(state) => {
                    // State changes are handled by the player bar
                }
                PositionChanged(position) => {
                    // Update our internal position tracking
                    self.position = position;
                }
                EndOfStream => {
                    // End of stream is handled by the player bar
                }
                Error(error) => {
                    // Handle playback errors
                    eprintln!("Playback error: {}", error);
                }
            }
        }
    }

    /// Sends a playback event to the player bar.
    ///
    /// Forwards a playback event to the connected player bar component for UI updates.
    /// This method is primarily used internally but can be called externally for
    /// custom event handling.
    ///
    /// # Parameters
    ///
    /// * `event` - The playback event to send to the player bar
    pub fn send_event(&self, event: PlaybackEvent) {
        // In a real implementation, this would send the event to the player bar
        // For now, we'll just print the event
    }

    /// Gets the current playback state.
    ///
    /// Returns a reference to the current playback state of the engine.
    ///
    /// # Returns
    ///
    /// A reference to the current [`PlaybackState`].
    pub fn get_current_state(&self) -> &PlaybackState {
        &self.engine.current_state
    }

    /// Gets the path of the currently loaded track.
    ///
    /// Returns an optional reference to the path of the currently loaded track.
    ///
    /// # Returns
    ///
    /// `Some(&PathBuf)` if a track is loaded, `None` otherwise.
    pub fn get_current_track(&self) -> Option<&PathBuf> {
        self.current_track.as_ref()
    }

    /// Gets the duration of the currently loaded track.
    ///
    /// Returns the duration of the currently loaded track in nanoseconds, if available.
    ///
    /// # Returns
    ///
    /// `Some(u64)` with the duration in nanoseconds if available, `None` otherwise.
    pub fn get_duration(&self) -> Option<u64> {
        self.duration
    }

    /// Gets the current playback position.
    ///
    /// Returns the current playback position in nanoseconds based on internal tracking.
    /// For the most up-to-date position from the engine, use [`query_position`](Self::query_position).
    ///
    /// # Returns
    ///
    /// The current playback position in nanoseconds.
    pub fn get_position(&self) -> u64 {
        self.position
    }

    /// Queries the current playback position from the engine.
    ///
    /// Requests the current playback position directly from the playback engine,
    /// which queries the underlying GStreamer pipeline.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing `Ok(Some(u64))` with the position in nanoseconds
    /// if successful, `Ok(None)` if the position is not available, or a [`PlaybackError`]
    /// if querying the position failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the playback engine fails to query the position.
    pub fn query_position(&mut self) -> Result<Option<u64>, PlaybackError> {
        self.engine.get_position()
    }
}

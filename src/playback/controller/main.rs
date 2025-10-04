use std::{path::PathBuf, sync::Arc};

use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::playback::{
    engine::PlaybackEngine,
    error::PlaybackError::{self, FileNotFound},
    events::{PlaybackEvent, PlaybackEvent::SongChanged, PlaybackState},
    queue::PlaybackQueue,
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
/// * `current_song` - Path to thecurrently loaded song, if any
/// * `duration` - Duration of the current song in nanoseconds, if available
/// * `position` - Current playback position in nanoseconds
/// * `queue` - The playback queue managing songs to be played
pub struct PlaybackController {
    /// The playback engine responsible for actual audio operations
    pub engine: PlaybackEngine,
    /// Sender for playback events to the engine
    pub event_sender: UnboundedSender<PlaybackEvent>,
    /// Receiver for playback events from the engine
    pub event_receiver: UnboundedReceiver<PlaybackEvent>,
    /// Path to the currently loaded song, if any
    pub current_song: Option<PathBuf>,
    /// Duration of the current song in nanoseconds, if available
    pub duration: Option<u64>,
    /// Current playback position in nanoseconds
    pub position: u64,
    /// The playback queue managing songs to be played
    pub queue: PlaybackQueue,
    /// Database connection pool for fetching album and song information
    pub db_pool: Arc<SqlitePool>,
}

impl PlaybackController {
    /// Creates a new playback controller.
    ///
    /// Initializes a new [`PlaybackController`].
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a tuple with:
    /// * The new `PlaybackController` instance
    /// * A `UnboundedSender<PlaybackEvent>` for sending events to the controller
    ///
    /// # Errors
    ///
    /// Returns a [`PlaybackError`] if the playback engine fails to initialize.
    pub fn new(
        db_pool: Arc<SqlitePool>,
    ) -> Result<(Self, UnboundedSender<PlaybackEvent>), PlaybackError> {
        let (event_sender, event_receiver) = unbounded_channel();
        let engine = PlaybackEngine::new(event_sender.clone())?;
        let controller = Self {
            engine,
            event_sender: event_sender.clone(),
            event_receiver,
            current_song: None,
            duration: None,
            position: 0,
            queue: PlaybackQueue::new(),
            db_pool,
        };
        Ok((controller, event_sender))
    }

    /// Loads a song for playback.
    ///
    /// Prepares the specified audio file for playback by loading it into the
    /// playback engine and querying its duration. The song is not automatically
    /// played; use [`play`](Self::play) to start playback.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to the audio file to load
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the song was successfully loaded, or a [`PlaybackError`]
    /// if loading failed.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * The file at `path` does not exist ([`FileNotFound`](PlaybackError::FileNotFound))
    /// * The playback engine fails to load the song
    /// * The duration query fails
    pub fn load_song(&mut self, path: PathBuf) -> Result<(), PlaybackError> {
        // Check if the file exists before trying to load it
        if !path.exists() {
            println!("Playback controller: File not found: {:?}", path);
            return Err(FileNotFound(path.clone()));
        }
        self.engine.load_song(&path)?;
        self.current_song = Some(path.clone());

        // Query the duration from GStreamer
        self.duration = self.engine.get_duration()?;

        // Send a SongChanged event if there's a current song in the queue
        if let Some(song_item) = self.queue.current_song() {
            let event = SongChanged(Box::new(song_item.clone()));
            if self.event_sender.send(event).is_err() {
                eprintln!("Failed to send SongChanged event");
            }
        } else {
            // If there's no current song in the queue but we're loading a song,
            // we might be in a state where the queue was set up but not yet processed
            // Let's try to ensure the current song is properly set in the queue
            eprintln!("Warning: No current song found in queue when loading song");
        }
        Ok(())
    }

    /// Starts playback of the currently loaded song.
    ///
    /// Initiates playback of the song that was previously loaded with
    /// [`load_song`](Self::load_song). If no song is loaded, this method
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
        self.engine.play()
    }

    /// Pauses playback of the currently playing song.
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
    /// Stops playback and resets the position to the beginning of the song.
    /// To resume playback, the song must be reloaded with [`load_song`](Self::load_song)
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

    /// Seeks to a specific position in the currently loaded song.
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
}

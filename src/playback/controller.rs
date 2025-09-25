use std::{path::PathBuf, sync::Arc};

use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::data::db::crud::{fetch_album_by_id, fetch_artist_by_id, fetch_tracks_by_album};

use super::{
    engine::PlaybackEngine,
    error::PlaybackError::{self, DatabaseError, FileNotFound},
    events::{
        PlaybackEvent::{self, EndOfStream, Error, PositionChanged, StateChanged, TrackChanged},
        PlaybackState,
    },
    queue::{PlaybackQueue, QueueItem},
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
/// * `current_track` - Path to thecurrently loaded track, if any
/// * `duration` - Duration of the current track in nanoseconds, if available
/// * `position` - Current playback position in nanoseconds
/// * `queue` - The playback queue managing tracks to be played
pub struct PlaybackController {
    /// The playback engine responsible for actual audio operations
    engine: PlaybackEngine,
    /// Sender for playback events to the engine
    event_sender: UnboundedSender<PlaybackEvent>,
    /// Receiver for playback events from the engine
    event_receiver: UnboundedReceiver<PlaybackEvent>,
    /// Path to the currently loaded track, if any
    current_track: Option<PathBuf>,
    /// Duration of the current track in nanoseconds, if available
    duration: Option<u64>,
    /// Current playback position in nanoseconds
    position: u64,
    /// The playback queue managing tracks to be played
    queue: PlaybackQueue,
    /// Database connection pool for fetching album and track information
    db_pool: Arc<SqlitePool>,
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
    pub fn new(db_pool: Arc<SqlitePool>) -> Result<(Self, UnboundedSender<PlaybackEvent>), PlaybackError> {
        let (event_sender, event_receiver) = unbounded_channel();
        let engine = PlaybackEngine::new(event_sender.clone())?;
        let controller = Self {
            engine,
            event_sender: event_sender.clone(),
            event_receiver,
            current_track: None,
            duration: None,
            position: 0,
            queue: PlaybackQueue::new(),
            db_pool,
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

        // If there's a current track in the queue, send a TrackChanged event
        if let Some(track_item) = self.queue.current_track() {
            let event = TrackChanged(Box::new(track_item.clone()));
            if self.event_sender.send(event).is_err() {
                eprintln!("Failed to send TrackChanged event");
            }
        }
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
        self.engine.play()
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

    /// Gets a mutable reference to the event receiver for async event handling
    ///
    /// This allows external components to await events from the playback engine
    /// using an async approach instead of polling.
    ///
    /// # Returns
    ///
    /// A mutable reference to the UnboundedReceiver<PlaybackEvent>
    pub fn get_event_receiver(&mut self) -> &mut UnboundedReceiver<PlaybackEvent> {
        &mut self.event_receiver
    }

    /// Waits for and handles the next incoming playback event from the engine.
    ///
    /// Processes the next event from the playback engine, updating internal
    /// state and acting on it (e.g., playing the next track on EndOfStream).
    ///
    /// # Returns
    ///
    /// A `PlaybackEvent` when one is received.
    pub async fn wait_for_next_event(&mut self) -> PlaybackEvent {
        if let Ok(event) = self.event_receiver.try_recv() {
            self.process_event(event.clone());
            event
        } else {
            // If no event is immediately available, await the next one
            let event = self.event_receiver.recv().await
                .expect("Event channel closed unexpectedly");
            self.process_event(event.clone());
            event
        }
    }

    /// Attempts to get an event from the receiver without blocking.
    ///
    /// # Returns
    ///
    /// An `Option<PlaybackEvent>` containing the event if one was available,
    /// or `None` if no event was immediately available.
    pub fn try_get_event(&mut self) -> Option<PlaybackEvent> {
        match self.event_receiver.try_recv() {
            Ok(event) => {
                self.process_event(event.clone());
                Some(event)
            }

            // No event available
            Err(_) => None,
        }
    }

    /// Processes a playback event and updates internal state
    ///
    /// This method handles the internal processing of playback events,
    /// updating the controller's state as needed.
    ///
    /// # Parameters
    ///
    /// * `event` - The playback event to process
    fn process_event(&mut self, event: PlaybackEvent) {
        match &event {
            TrackChanged(_) => {
                // Metadata changes are handled by the player bar
            }
            StateChanged(_state) => {
                // State changes are handled by the player bar
            }
            PositionChanged(position) => {
                // Update our internal position tracking
                self.position = *position;
            }
            EndOfStream => {
                // When the current track ends, try to play the next track in the queue
                if let Err(e) = self.next_track() {
                    eprintln!("Error playing next track: {}", e);
                }
            }
            Error(error) => {
                // Handle playback errors
                eprintln!("Playback error: {}", error);
            }
        }
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

    /// Checks if navigation to the next track is possible
    ///
    /// Returns true if there is a next track in the queue, false otherwise
    pub fn can_go_next(&self) -> bool {
        self.queue.can_go_next()
    }

    /// Checks if navigation to the previous track is possible
    ///
    /// Returns true if there is a previous track in the queue, false otherwise
    pub fn can_go_previous(&self) -> bool {
        self.queue.can_go_previous()
    }

    /// Queues all tracks from an album for playback
    ///
    /// This method fetches album, artist, and track information from the database,
    /// creates QueueItem objects for each track, clears the existing queue,
    /// adds the new items, sets the current album ID and index, and loads and plays
    /// the first track.
    ///
    /// # Arguments
    /// * `album_id` - The ID of the album to queue
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub async fn queue_album(&mut self, album_id: i64) -> Result<(), PlaybackError> {
        // Fetch album information
        let album = fetch_album_by_id(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch album: {}", e)))?;

        // Fetch artist information
        let artist = fetch_artist_by_id(&self.db_pool, album.artist_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch artist: {}", e)))?;

        // Fetch tracks for the album
        let tracks = fetch_tracks_by_album(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch tracks: {}", e)))?;

        // Clear existing queue
        self.queue.clear();

        // Create QueueItem for each track
        let queue_items: Vec<QueueItem> = tracks
            .into_iter()
            .map(|track| QueueItem {
                track_title: track.title,
                album_title: album.title.clone(),
                artist_name: artist.name.clone(),
                track_path: track.path,
                cover_art_path: album.cover_art.clone(),
                bit_depth: track.bit_depth,
                sample_rate: track.sample_rate,
                format: track.format,
                duration: track.duration,
            })
            .collect();

        // Add new items to queue
        self.queue.items = queue_items;

        // Set current album ID and index
        self.queue.current_album_id = Some(album_id);
        self.queue.current_index = if self.queue.items.is_empty() {
            None
        } else {
            Some(0)
        };

        // Load and play the first track if there are tracks
        if let Some(first_track) = self.queue.current_track() {
            self.load_track(first_track.track_path.clone())?;
            self.play()?;
        }
        Ok(())
    }

    /// Queues all tracks from an album, starting playback from a specific track
    ///
    /// This method fetches album, artist, and track information from the database,
    /// creates QueueItem objects for all tracks in the album, clears the existing queue,
    /// adds all items to the queue, sets the current album ID and index to the selected track,
    /// and loads and plays the selected track.
    ///
    /// # Arguments
    /// * `album_id` - The ID of the album
    /// * `start_track_id` - The ID of the track to start playing from
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub async fn queue_tracks_from(
        &mut self,
        album_id: i64,
        start_track_id: i64,
    ) -> Result<(), PlaybackError> {
        // Fetch album information
        let album = fetch_album_by_id(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch album: {}", e)))?;

        // Fetch artist information
        let artist = fetch_artist_by_id(&self.db_pool, album.artist_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch artist: {}", e)))?;

        // Fetch tracks for the album
        let tracks = fetch_tracks_by_album(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch tracks: {}", e)))?;

        // Find the starting track position
        let start_index = tracks
            .iter()
            .position(|track| track.id == start_track_id)
            .ok_or_else(|| DatabaseError("Start track not found in album".to_string()))?;

        // Clear existing queue
        self.queue.clear();

        // Create QueueItem for each track in the album
        let queue_items: Vec<QueueItem> = tracks
            .iter()
            .map(|track| QueueItem {
                track_title: track.title.clone(),
                album_title: album.title.clone(),
                artist_name: artist.name.clone(),
                track_path: track.path.clone(),
                cover_art_path: album.cover_art.clone(),
                bit_depth: track.bit_depth,
                sample_rate: track.sample_rate,
                format: track.format.clone(),
                duration: track.duration,
            })
            .collect();

        // Add all items to queue
        self.queue.items = queue_items;

        // Set current album ID and index to the selected track
        self.queue.current_album_id = Some(album_id);
        self.queue.current_index = if self.queue.items.is_empty() {
            None
        } else {
            Some(start_index)
        };

        // Load and play the selected track if there are tracks
        if let Some(selected_track) = self.queue.current_track() {
            self.load_track(selected_track.track_path.clone())?;
            self.play()?;
        }
        Ok(())
    }

    /// Plays the next track in the queue
    ///
    /// This method checks if there is a next track, increments the current index,
    /// and loads and plays the next track.
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub fn next_track(&mut self) -> Result<(), PlaybackError> {
        // Get current index
        let current_index = self.queue.current_index;

        // Check if there is a next track
        if let Some(index) = current_index {
            if index + 1 < self.queue.items.len() {
                // Increment current index
                self.queue.current_index = Some(index + 1);

                // Get the next track
                if let Some(next_track) = self.queue.current_track() {
                    // Load and play the next track
                    self.load_track(next_track.track_path.clone())?;
                    self.play()?;
                    return Ok(());
                } else {
                    println!("Controller: No next track found");
                }
            }
        } else {
            println!("Controller: Current index is None");
        }

        // No next track, stop playback
        self.stop()?;
        Ok(())
    }

    /// Plays the previous track in the queue
    ///
    /// This method checks if there is a previous track, decrements the current index,
    /// and loads and plays the previous track.
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub fn previous_track(&mut self) -> Result<(), PlaybackError> {
        // Get current index
        let current_index = self.queue.current_index;

        // Check if there is a previous track
        if let Some(index) = current_index {
            if index > 0 {
                // Decrement current index
                self.queue.current_index = Some(index - 1);

                // Get the previous track
                if let Some(prev_track) = self.queue.current_track() {
                    // Load and play the previous track
                    self.load_track(prev_track.track_path.clone())?;
                    self.play()?;
                    return Ok(());
                }
            } else {
                // No previous track, just restart current track from beginning
                self.stop()?;
                if let Some(current_track) = self.queue.current_track() {
                    self.load_track(current_track.track_path.clone())?;
                    self.play()?;
                }
                return Ok(());
            }
        }
        Ok(())
    }
}

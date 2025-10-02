use std::{path::PathBuf, sync::Arc};

use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::data::db::crud::{fetch_album_by_id, fetch_artist_by_id, fetch_songs_by_album};

use super::{
    engine::PlaybackEngine,
    error::PlaybackError::{self, DatabaseError, FileNotFound},
    events::{
        PlaybackEvent::{self, EndOfStream, Error, PositionChanged, SongChanged, StateChanged},
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
/// * `current_song` - Path to thecurrently loaded song, if any
/// * `duration` - Duration of the current song in nanoseconds, if available
/// * `position` - Current playback position in nanoseconds
/// * `queue` - The playback queue managing songs to be played
pub struct PlaybackController {
    /// The playback engine responsible for actual audio operations
    engine: PlaybackEngine,
    /// Sender for playback events to the engine
    event_sender: UnboundedSender<PlaybackEvent>,
    /// Receiver for playback events from the engine
    event_receiver: UnboundedReceiver<PlaybackEvent>,
    /// Path to the currently loaded song, if any
    current_song: Option<PathBuf>,
    /// Duration of the current song in nanoseconds, if available
    duration: Option<u64>,
    /// Current playback position in nanoseconds
    position: u64,
    /// The playback queue managing songs to be played
    queue: PlaybackQueue,
    /// Database connection pool for fetching album and song information
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
            SongChanged(_) => {
                // Metadata changes are handled by the player bar
            }
            StateChanged(_state) => {
                // State changes are handled by the player bar
            }
            PositionChanged(position) => {
                // Update our internal position songing
                self.position = *position;
            }
            EndOfStream => {
                // When the current song ends, try to play the next song in the queue
                if let Err(e) = self.next_song() {
                    eprintln!("Error playing next song: {}", e);
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

    /// Checks if navigation to the next song is possible
    ///
    /// Returns true if there is a next song in the queue, false otherwise
    pub fn can_go_next(&self) -> bool {
        self.queue.can_go_next()
    }

    /// Checks if navigation to the previous song is possible
    ///
    /// Returns true if there is a previous song in the queue, false otherwise
    pub fn can_go_previous(&self) -> bool {
        self.queue.can_go_previous()
    }

    /// Queues all songs from an album for playback
    ///
    /// This method fetches album, artist, and song information from the database,
    /// creates QueueItem objects for each song, clears the existing queue,
    /// adds the new items, sets the current album ID and index, and loads and plays
    /// the first song.
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

        // Fetch songs for the album
        let songs = fetch_songs_by_album(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch songs: {}", e)))?;

        // Clear existing queue
        self.queue.clear();

        // Create QueueItem for each song
        let queue_items: Vec<QueueItem> = songs
            .into_iter()
            .map(|song| QueueItem {
                song_title: song.title,
                album_title: album.title.clone(),
                artist_name: artist.name.clone(),
                song_path: song.path,
                cover_art_path: album.cover_art.clone(),
                bit_depth: song.bit_depth,
                sample_rate: song.sample_rate,
                format: song.format,
                duration: song.duration,
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

        // Load and play the first song if there are songs
        if let Some(first_song) = self.queue.current_song() {
            self.load_song(first_song.song_path.clone())?;
            self.play()?;
        }
        Ok(())
    }

    /// Queues all songs from an album, starting playback from a specific song
    ///
    /// This method fetches album, artist, and song information from the database,
    /// creates QueueItem objects for all songs in the album, clears the existing queue,
    /// adds all items to the queue, sets the current album ID and index to the selected song,
    /// and loads and plays the selected song.
    ///
    /// # Arguments
    /// * `album_id` - The ID of the album
    /// * `start_song_id` - The ID of the song to start playing from
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub async fn queue_songs_from(
        &mut self,
        album_id: i64,
        start_song_id: i64,
    ) -> Result<(), PlaybackError> {
        // Fetch album information
        let album = fetch_album_by_id(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch album: {}", e)))?;

        // Fetch artist information
        let artist = fetch_artist_by_id(&self.db_pool, album.artist_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch artist: {}", e)))?;

        // Fetch songs for the album
        let songs = fetch_songs_by_album(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch songs: {}", e)))?;

        // Find the starting song position
        let start_index = songs
            .iter()
            .position(|song| song.id == start_song_id)
            .ok_or_else(|| DatabaseError("Start song not found in album".to_string()))?;

        // Clear existing queue
        self.queue.clear();

        // Create QueueItem for each song in the album
        let queue_items: Vec<QueueItem> = songs
            .iter()
            .map(|song| QueueItem {
                song_title: song.title.clone(),
                album_title: album.title.clone(),
                artist_name: artist.name.clone(),
                song_path: song.path.clone(),
                cover_art_path: album.cover_art.clone(),
                bit_depth: song.bit_depth,
                sample_rate: song.sample_rate,
                format: song.format.clone(),
                duration: song.duration,
            })
            .collect();

        // Add all items to queue
        self.queue.items = queue_items;

        // Set current album ID and index to the selected song
        self.queue.current_album_id = Some(album_id);
        self.queue.current_index = if self.queue.items.is_empty() {
            None
        } else {
            Some(start_index)
        };

        // Load and play the selected song if there are songs
        if let Some(selected_song) = self.queue.current_song() {
            self.load_song(selected_song.song_path.clone())?;
            self.play()?;
        }
        Ok(())
    }

    /// Plays the next song in the queue
    ///
    /// This method checks if there is a next song, increments the current index,
    /// and loads and plays the next song.
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub fn next_song(&mut self) -> Result<(), PlaybackError> {
        // Get current index
        let current_index = self.queue.current_index;

        // Check if there is a next song
        if let Some(index) = current_index {
            if index + 1 < self.queue.items.len() {
                // Increment current index
                self.queue.current_index = Some(index + 1);

                // Get the next song
                if let Some(next_song) = self.queue.current_song() {
                    // Load and play the next song
                    self.load_song(next_song.song_path.clone())?;
                    self.play()?;
                    return Ok(());
                } else {
                    println!("Controller: No next song found");
                }
            }
        } else {
            println!("Controller: Current index is None");
        }

        // No next song, stop playback
        self.stop()?;
        Ok(())
    }

    /// Plays the previous song in the queue
    ///
    /// This method checks if there is a previous song, decrements the current index,
    /// and loads and plays the previous song.
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub fn previous_song(&mut self) -> Result<(), PlaybackError> {
        // Get current index
        let current_index = self.queue.current_index;

        // Check if there is a previous song
        if let Some(index) = current_index {
            if index > 0 {
                // Decrement current index
                self.queue.current_index = Some(index - 1);

                // Get the previous song
                if let Some(prev_song) = self.queue.current_song() {
                    // Load and play the previous song
                    self.load_song(prev_song.song_path.clone())?;
                    self.play()?;
                    return Ok(());
                }
            } else {
                // No previous song, just restart current song from beginning
                self.stop()?;
                if let Some(current_song) = self.queue.current_song() {
                    self.load_song(current_song.song_path.clone())?;
                    self.play()?;
                }
                return Ok(());
            }
        }
        Ok(())
    }

    /// Gets the previous song information from the queue
    ///
    /// # Returns
    /// An `Option<QueueItem>` containing the previous song information if available
    pub fn get_previous_song_info(&self) -> Option<QueueItem> {
        if let Some(current_index) = self.queue.current_index {
            if current_index > 0 && current_index <= self.queue.items.len() {
                self.queue.items.get(current_index - 1).cloned()
            } else {
                // If at the first song, return the current song (for restart behavior)
                self.queue.current_song().cloned()
            }
        } else {
            None
        }
    }

    /// Gets the next song information from the queue
    ///
    /// # Returns
    /// An `Option<QueueItem>` containing the next song information if available
    pub fn get_next_song_info(&self) -> Option<QueueItem> {
        if let Some(current_index) = self.queue.current_index {
            if current_index + 1 < self.queue.items.len() {
                self.queue.items.get(current_index + 1).cloned()
            } else {
                None
            }
        } else {
            None
        }
    }
}

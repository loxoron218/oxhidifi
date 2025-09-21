use std::path::PathBuf;

/// Represents a single item in the playback queue.
#[derive(Clone, Debug)]
pub struct QueueItem {
    /// Unique identifier for the track
    pub track_id: i64,
    /// Title of the track
    pub track_title: String,
    /// Title of the album containing the track
    pub album_title: String,
    /// Name of the artist performing the track
    pub artist_name: String,
    /// File system path to the track audio file
    pub track_path: PathBuf,
    /// Optional path to the album cover art
    pub cover_art_path: Option<PathBuf>,
    /// Optional bit depth of the audio file
    pub bit_depth: Option<u32>,
    /// Optional sample rate of the audio file
    pub sample_rate: Option<u32>,
    /// Optional format of the audio file (e.g., "FLAC", "MP3")
    pub format: Option<String>,
    /// Optional duration of the track in seconds
    pub duration: Option<u32>,
}

/// Manages the queue of tracks and the current playback position.
#[derive(Clone, Debug)]
pub struct PlaybackQueue {
    /// Vector of QueueItem objects representing the tracks in the queue
    pub items: Vec<QueueItem>,
    /// Optional index of the currently playing track in the queue
    pub current_index: Option<usize>,
    /// Optional ID of the album currently being played (used to detect when a new album is played)
    pub current_album_id: Option<i64>,
}

impl PlaybackQueue {
    /// Creates a new empty playback queue
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_index: None,
            current_album_id: None,
        }
    }

    /// Clears the queue and resets all state
    pub fn clear(&mut self) {
        self.items.clear();
        self.current_index = None;
        self.current_album_id = None;
    }

    /// Checks if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Gets the number of items in the queue
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Gets a reference to the current track, if any
    pub fn current_track(&self) -> Option<&QueueItem> {
        if let Some(index) = self.current_index {
            self.items.get(index)
        } else {
            None
        }
    }

    /// Gets a reference to the next track in the queue, if any
    pub fn next_track(&self) -> Option<&QueueItem> {
        if let Some(index) = self.current_index {
            if index + 1 < self.items.len() {
                return self.items.get(index + 1);
            }
        }
        None
    }

    /// Checks if navigation to the next track is possible
    pub fn can_go_next(&self) -> bool {
        if let Some(index) = self.current_index {
            index + 1 < self.items.len()
        } else {
            false
        }
    }

    /// Gets a reference to the previous track in the queue, if any
    pub fn previous_track(&self) -> Option<&QueueItem> {
        if let Some(index) = self.current_index {
            if index > 0 {
                return self.items.get(index - 1);
            }
        }
        None
    }

    /// Checks if navigation to the previous track is possible
    pub fn can_go_previous(&self) -> bool {
        if let Some(index) = self.current_index {
            index > 0
        } else {
            false
        }
    }
}

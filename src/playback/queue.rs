use std::path::PathBuf;

/// Represents a single item in the playback queue.
#[derive(Clone, Debug)]
pub struct QueueItem {
    /// Title of the song
    pub song_title: String,
    /// Title of the album containing the song
    pub album_title: String,
    /// Name of the artist performing the song
    pub artist_name: String,
    /// File system path to the song audio file
    pub song_path: PathBuf,
    /// Optional path to the album cover art
    pub cover_art_path: Option<PathBuf>,
    /// Optional bit depth of the audio file
    pub bit_depth: Option<u32>,
    /// Optional sample rate of the audio file
    pub sample_rate: Option<u32>,
    /// Optional format of the audio file (e.g., "FLAC", "MP3")
    pub format: Option<String>,
    /// Optional duration of the song in seconds
    pub duration: Option<u32>,
}

/// Manages the queue of songs and the current playback position.
#[derive(Clone, Debug)]
pub struct PlaybackQueue {
    /// Vector of QueueItem objects representing the songs in the queue
    pub items: Vec<QueueItem>,
    /// Optional index of the currently playing song in the queue
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

    /// Gets a reference to the current song, if any
    pub fn current_song(&self) -> Option<&QueueItem> {
        if let Some(index) = self.current_index {
            self.items.get(index)
        } else {
            None
        }
    }

    /// Checks if navigation to the next song is possible
    pub fn can_go_next(&self) -> bool {
        if let Some(index) = self.current_index {
            index + 1 < self.items.len()
        } else {
            false
        }
    }

    /// Checks if navigation to the previous song is possible
    pub fn can_go_previous(&self) -> bool {
        if let Some(index) = self.current_index {
            index > 0
        } else {
            false
        }
    }
}

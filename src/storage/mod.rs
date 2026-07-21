//! Persistence layer: domain types, storage trait, and error types.

pub mod database;
pub mod migrations;
pub mod settings;

use std::{borrow::Cow, collections::HashMap, future::Future, path::Path, result::Result};

use {sqlx::FromRow, thiserror::Error};

use crate::playback::layout::{AudioLayout, format_channel_label};

/// Full album record from the database.
#[derive(Debug, Clone, FromRow)]
pub struct Album {
    /// Unique album identifier.
    pub id: i64,
    /// Album title.
    pub title: String,
    /// Foreign key to artist.
    pub artist_id: i64,
    /// Release year.
    pub year: Option<i32>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Path to cached album artwork.
    pub artwork_path: Option<String>,
    /// Number of tracks.
    pub track_count: i32,
    /// Total duration in seconds.
    pub total_duration: f64,
    /// Format description string.
    pub format_summary: String,
    /// Whether all tracks are lossless.
    pub lossless: bool,
    /// Audio codec name (e.g. "FLAC", "MP3").
    pub format: String,
    /// Bit depth (None for lossy formats).
    pub bit_depth: Option<i32>,
    /// Sample rate in Hz.
    pub sample_rate: Option<i32>,
}

/// Full artist record from the database.
#[derive(Debug, Clone, FromRow)]
pub struct Artist {
    /// Unique artist identifier.
    pub id: i64,
    /// Artist name.
    pub name: String,
    /// Number of albums by this artist.
    pub album_count: i32,
}

/// Represents the intent for a nullable database field in an update operation.
#[derive(Debug, Clone, Default)]
pub enum FieldUpdate<T> {
    /// Do not touch this field.
    #[default]
    Skip,
    /// Set the field to SQL NULL.
    SetNull,
    /// Set the field to the given value.
    Set(T),
}

/// Distinct format values for an album, computed from its tracks.
#[derive(Debug, Clone, Default)]
pub struct FormatInfo {
    /// Distinct format/codec names (uppercased).
    pub formats: Vec<String>,
    /// Distinct sample rates in Hz.
    pub sample_rates: Vec<i32>,
    /// Distinct bit depths.
    pub bit_depths: Vec<i32>,
    /// Distinct channel counts.
    pub channels: Vec<i32>,
}

impl FormatInfo {
    /// Whether all tracks share the same format properties.
    #[must_use]
    pub fn is_uniform(&self) -> bool {
        self.formats.len() <= 1 && self.sample_rates.len() <= 1 && self.bit_depths.len() <= 1
    }

    /// Compact summary for album **grid cards** (no units, no bullets).
    ///
    /// Order is always: format(s) → bit-depth(s) → sample-rate(s).
    /// Bit depth and sample rate are joined with `/` when both present.
    ///
    /// Uniform lossless: `"FLAC 24/96"`
    /// Uniform lossy:    `"MP3 44.1"`
    /// Mixed:           `"FLAC, MP3 16, 24/44.1, 96"`
    #[must_use]
    pub fn summary(&self) -> String {
        let fmt = self.formats_display();
        let bd = self
            .bit_depths
            .iter()
            .map(|&b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let sr = self
            .sample_rates
            .iter()
            .map(|&hz| format_sample_rate_str(hz))
            .collect::<Vec<_>>()
            .join(", ");

        let mut parts: Vec<String> = Vec::new();
        if !fmt.is_empty() {
            parts.push(fmt);
        }
        match (!bd.is_empty(), !sr.is_empty()) {
            (true, true) => parts.push(format!("{bd}/{sr}")),
            (false, true) => parts.push(sr),
            (true, false) => parts.push(bd),
            (false, false) => {}
        }
        parts.join(" ")
    }

    /// Full summary for **detail pages** (with units and channels, matches side panel).
    ///
    /// Format first, then bit depth + sample rate grouped with ` / ` when both
    /// present, then channel label. Sample rates always show one decimal place
    /// to match the side panel (e.g. `96.0 kHz`).
    ///
    /// Uniform lossless: `"FLAC \u{2022} 24-bit / 96.0 kHz \u{2022} Stereo"`
    /// Uniform lossy:    `"MP3 \u{2022} 44.1 kHz \u{2022} Stereo"`
    /// Mixed:           `"FLAC, MP3 \u{2022} 16, 24-bit / 44.1, 96.0 kHz \u{2022} Stereo"`
    #[must_use]
    pub fn summary_detailed(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let fmt = self.formats_display();
        if !fmt.is_empty() {
            parts.push(fmt);
        }
        let bd = self.bit_depth_display();
        let sr = self.sample_rate_display();
        match (!bd.is_empty(), !sr.is_empty()) {
            (true, true) => parts.push(format!("{bd} / {sr}")),
            (true, false) => parts.push(bd),
            (false, true) => parts.push(sr),
            (false, false) => {}
        }
        if !self.channels.is_empty() {
            let ch: Vec<Cow<'static, str>> =
                self.channels.iter().copied().map(fmt_channel).collect();
            parts.push(ch.join(", "));
        }
        parts.join(" \u{2022} ")
    }

    /// Display string for the format column in column view.
    #[must_use]
    pub fn formats_display(&self) -> String {
        if self.formats.is_empty() {
            String::new()
        } else {
            self.formats.join(", ")
        }
    }

    /// Display string for sample rates. Always shows one decimal place
    /// (e.g. `96.0 kHz`) to match the side panel formatting.
    #[must_use]
    pub fn sample_rate_display(&self) -> String {
        if self.sample_rates.is_empty() {
            String::new()
        } else {
            let srs: Vec<String> = self
                .sample_rates
                .iter()
                .map(|&hz| format!("{:.1}", f64::from(hz) / 1000.0))
                .collect();
            format!("{} kHz", srs.join(", "))
        }
    }

    /// Display string for the bit depth column in column view.
    #[must_use]
    pub fn bit_depth_display(&self) -> String {
        if self.bit_depths.is_empty() {
            String::new()
        } else {
            let bds: Vec<String> = self.bit_depths.iter().map(|&b| b.to_string()).collect();
            format!("{}-bit", bds.join(", "))
        }
    }
}

/// A configured library directory.
#[derive(Debug, Clone, FromRow)]
pub struct LibraryDirectory {
    /// Unique directory identifier.
    pub id: i64,
    /// Absolute filesystem path.
    pub path: String,
    /// Whether this directory is actively watched.
    pub enabled: bool,
    /// Timestamp of last completed scan.
    pub last_scanned: Option<String>,
    /// When directory was added.
    pub added_at: String,
}

/// Insert data for a new album.
#[derive(Debug, Clone)]
pub struct NewAlbum {
    /// Album title.
    pub title: String,
    /// Foreign key to artist.
    pub artist_id: i64,
    /// Release year.
    pub year: Option<i32>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Path to cached album artwork.
    pub artwork_path: Option<String>,
    /// Format description string.
    pub format_summary: String,
    /// Whether all tracks are lossless.
    pub lossless: bool,
    /// Audio codec name (e.g. "FLAC", "MP3").
    pub format: String,
    /// Bit depth (None for lossy formats).
    pub bit_depth: Option<i32>,
    /// Sample rate in Hz.
    pub sample_rate: Option<i32>,
}

/// Insert data for a new artist.
#[derive(Debug, Clone)]
pub struct NewArtist {
    /// Artist name.
    pub name: String,
}

/// Insert data for a new queue entry.
#[derive(Debug, Clone)]
pub struct NewQueueEntry {
    /// Foreign key to track.
    pub track_id: i64,
    /// Position in queue (0 = next to play).
    pub position: i32,
    /// How the track was queued ("album", "artist", "manual").
    pub context_type: Option<String>,
    /// Id of album/artist context.
    pub context_id: Option<i64>,
}

/// Insert data for a new track.
#[derive(Debug, Clone)]
pub struct NewTrack {
    /// Track title.
    pub title: String,
    /// Track number within album/disc.
    pub track_number: Option<i32>,
    /// Disc number.
    pub disc_number: Option<i32>,
    /// Duration in seconds.
    pub duration: f64,
    /// Audio file metadata.
    pub audio: TrackAudio,
}

/// Context describing how a track was added to the queue.
#[derive(Debug, Clone)]
pub enum QueueContext {
    /// Queued from an album context.
    Album(i64),
    /// Queued from an artist context.
    Artist(i64),
    /// Manually queued.
    Manual,
}

/// Unique queue entry with database id.
#[derive(Debug, Clone, FromRow)]
pub struct QueueEntry {
    /// Database id of this queue entry.
    pub id: i64,
    /// Foreign key to Track.
    pub track_id: i64,
    /// Position in queue (0 = next to play).
    pub position: i32,
    /// How the track was queued ("album", "artist", "manual").
    pub context_type: Option<String>,
    /// Id of album/artist context.
    pub context_id: Option<i64>,
    /// When this entry was added.
    pub added_at: String,
}

/// Interface for all persistent storage operations.
pub trait Storage: Send + Sync + 'static {
    /// Insert a new track, returning its id.
    fn insert_track(&self, track: NewTrack) -> impl Future<Output = StorageResult<i64>> + Send;

    /// Update an existing track.
    fn update_track(
        &self,
        id: i64,
        track: TrackUpdate,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Delete a track by id.
    fn delete_track(&self, id: i64) -> impl Future<Output = StorageResult<()>> + Send;

    /// Get a track by id.
    fn get_track(&self, id: i64) -> impl Future<Output = StorageResult<Option<Track>>> + Send;

    /// Get all tracks belonging to an album.
    fn get_tracks_by_album(
        &self,
        album_id: i64,
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Get all tracks by an artist.
    fn get_tracks_by_artist(
        &self,
        artist_id: i64,
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Search tracks by query string.
    fn search_tracks(&self, query: &str) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Insert a new album, returning its id.
    fn insert_album(&self, album: NewAlbum) -> impl Future<Output = StorageResult<i64>> + Send;

    /// Get an album by id.
    fn get_album(&self, id: i64) -> impl Future<Output = StorageResult<Option<Album>>> + Send;

    /// Get all albums.
    fn get_all_albums(&self) -> impl Future<Output = StorageResult<Vec<Album>>> + Send;

    /// Get distinct format info for a single album.
    fn get_album_format_info(
        &self,
        album_id: i64,
    ) -> impl Future<Output = StorageResult<FormatInfo>> + Send;

    /// Get distinct format info for multiple albums at once.
    fn get_albums_format_info(
        &self,
        album_ids: &[i64],
    ) -> impl Future<Output = StorageResult<HashMap<i64, FormatInfo>>> + Send;

    /// Get all albums by an artist.
    fn get_albums_by_artist(
        &self,
        artist_id: i64,
    ) -> impl Future<Output = StorageResult<Vec<Album>>> + Send;

    /// Insert a new artist, returning its id.
    fn insert_artist(&self, artist: NewArtist) -> impl Future<Output = StorageResult<i64>> + Send;

    /// Get an artist by id.
    fn get_artist(&self, id: i64) -> impl Future<Output = StorageResult<Option<Artist>>> + Send;

    /// Get all artists.
    fn get_all_artists(&self) -> impl Future<Output = StorageResult<Vec<Artist>>> + Send;

    /// List all configured library directories.
    fn list_library_directories(
        &self,
    ) -> impl Future<Output = StorageResult<Vec<LibraryDirectory>>> + Send;

    /// Add a library directory.
    fn add_library_directory(&self, path: &Path) -> impl Future<Output = StorageResult<()>> + Send;

    /// Remove a library directory by id.
    fn remove_library_directory(&self, id: i64) -> impl Future<Output = StorageResult<()>> + Send;

    /// Get the current playback queue.
    fn get_queue(&self) -> impl Future<Output = StorageResult<Vec<QueueEntry>>> + Send;

    /// Replace the entire queue.
    fn set_queue(
        &self,
        entries: &[NewQueueEntry],
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Append a track to the end of the queue.
    fn append_queue(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Remove a queue entry by id.
    fn remove_queue_entry(&self, id: i64) -> impl Future<Output = StorageResult<()>> + Send;

    /// Move a queue entry to a new position.
    fn reorder_queue(
        &self,
        entry_id: i64,
        new_position: u32,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Clear the entire queue.
    fn clear_queue(&self) -> impl Future<Output = StorageResult<()>> + Send;

    /// Find a track by file path.
    fn find_by_path(
        &self,
        path: &Path,
    ) -> impl Future<Output = StorageResult<Option<Track>>> + Send;

    /// Find tracks by content hash.
    fn find_by_hash(&self, hash: &str) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Find tracks by metadata fingerprint.
    fn find_by_metadata_fingerprint(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Insert multiple tracks in a batch, returning their ids.
    fn insert_tracks_batch(
        &self,
        tracks: Vec<NewTrack>,
    ) -> impl Future<Output = StorageResult<Vec<i64>>> + Send;

    /// Find tracks by multiple file paths in a batch.
    fn find_by_paths_batch(
        &self,
        paths: &[&Path],
    ) -> impl Future<Output = StorageResult<Vec<Option<Track>>>> + Send;

    /// Find tracks by multiple content hashes in a batch.
    fn find_by_hashes_batch(
        &self,
        hashes: &[&str],
    ) -> impl Future<Output = StorageResult<Vec<Vec<Track>>>> + Send;

    /// Get tracks belonging to multiple albums in a single query.
    fn get_tracks_by_albums(
        &self,
        album_ids: &[i64],
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Get multiple tracks by their IDs in a single query.
    fn get_tracks_by_ids(
        &self,
        ids: &[i64],
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;
}

/// Error type for storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Database error.
    #[error("Database error: {0}")]
    Database(String),
    /// Entity not found.
    #[error("Entity not found: {0}")]
    NotFound(String),
    /// Duplicate entry.
    #[error("Duplicate entry: {0}")]
    Duplicate(String),
    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),
    /// Invalid path.
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Convenience alias for storage operation results.
pub type StorageResult<T> = Result<T, StorageError>;

/// Full track record from the database.
#[derive(Debug, Clone, FromRow)]
pub struct Track {
    /// Unique track identifier.
    pub id: i64,
    /// Track title.
    pub title: String,
    /// Track number within album/disc.
    pub number: Option<i32>,
    /// Disc number.
    pub disc_number: Option<i32>,
    /// Duration in seconds.
    pub duration: f64,
    /// Audio file metadata.
    #[sqlx(flatten)]
    pub audio: TrackAudio,
    /// Database insertion time.
    pub created_at: String,
}

/// Audio metadata shared between insert and retrieval.
#[derive(Debug, Clone, FromRow)]
pub struct TrackAudio {
    /// Absolute path to audio file.
    pub file_path: String,
    /// SHA-256 hex digest.
    pub content_hash: Option<String>,
    /// File format (FLAC, MP3, etc.).
    pub format: String,
    /// Native sample rate in Hz.
    pub sample_rate: i32,
    /// Bit depth (none for lossy formats).
    pub bit_depth: Option<i32>,
    /// Number of audio channels.
    pub channels: i32,
    /// Codec identifier.
    pub codec: String,
    /// Whether format is lossless.
    pub lossless: bool,
    /// Average bitrate in kbps.
    pub bitrate: Option<i32>,
    /// Foreign key to album.
    pub album_id: Option<i64>,
    /// Foreign key to artist.
    pub artist_id: Option<i64>,
    /// File size in bytes.
    pub file_size: i64,
    /// Filesystem mtime at scan time.
    pub last_modified: String,
}

/// Partial update fields for a track.
#[derive(Debug, Clone, Default)]
pub struct TrackUpdate {
    /// New track title.
    pub title: Option<String>,
    /// New track number.
    pub track_number: FieldUpdate<i32>,
    /// New disc number.
    pub disc_number: FieldUpdate<i32>,
    /// New duration in seconds.
    pub duration: Option<f64>,
    /// New content hash.
    pub content_hash: FieldUpdate<String>,
    /// New album id.
    pub album_id: FieldUpdate<i64>,
    /// New artist id.
    pub artist_id: FieldUpdate<i64>,
}

/// Format a channel count to a human-readable label.
#[must_use]
fn fmt_channel(c: i32) -> Cow<'static, str> {
    format_channel_label(AudioLayout::from_count(u32::try_from(c).unwrap_or(0)))
}

/// Format a sample rate in Hz to a short kHz string.
#[must_use]
pub fn format_sample_rate_str(hz: i32) -> String {
    if hz % 1000 == 0 {
        (hz / 1000).to_string()
    } else {
        format!("{:.1}", f64::from(hz) / 1000.0)
    }
}

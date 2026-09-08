//! Persistence record, insert, and update types.

use sqlx::FromRow;

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
#[derive(Debug, Clone, Copy)]
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

#[cfg(test)]
mod tests {
    use crate::storage::catalog::{
        NewQueueEntry,
        QueueContext::{self, Album, Artist, Manual},
        QueueEntry,
    };

    #[test]
    fn new_queue_entry_fields_and_debug() {
        let entry = NewQueueEntry {
            track_id: 42,
            position: 3,
            context_type: Some("album".to_string()),
            context_id: Some(7),
        };
        assert_eq!(entry.track_id, 42, "track id must round-trip");
        assert_eq!(entry.position, 3, "position must round-trip");
        assert_eq!(
            entry.context_type.as_deref(),
            Some("album"),
            "context type must round-trip"
        );
        assert_eq!(entry.context_id, Some(7), "context id must round-trip");
        let cloned = entry.clone();
        assert_eq!(cloned.track_id, 42, "clone must preserve track id");
        assert!(
            format!("{entry:?}").contains("42"),
            "debug must include track id"
        );
    }

    #[test]
    fn queue_context_variants_copy_and_debug() {
        let album = Album(1);
        let artist = Artist(2);
        let manual = Manual;
        let copied: QueueContext = album;
        assert!(
            matches!(copied, Album(1)),
            "copied album context must match"
        );
        assert!(matches!(album, Album(1)), "album context must survive copy");
        assert!(matches!(artist, Artist(2)), "artist context must match");
        assert!(matches!(manual, Manual), "manual context must match");
        assert!(
            format!("{album:?}").contains('1'),
            "album debug must include id"
        );
        assert!(
            format!("{artist:?}").contains('2'),
            "artist debug must include id"
        );
        assert!(
            format!("{manual:?}").contains("Manual"),
            "manual debug must include variant"
        );
    }

    #[test]
    fn queue_entry_fields_and_debug() {
        let entry = QueueEntry {
            id: 1,
            track_id: 2,
            position: 0,
            context_type: None,
            context_id: None,
            added_at: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(entry.id, 1, "id must round-trip");
        assert_eq!(entry.track_id, 2, "track id must round-trip");
        assert_eq!(entry.position, 0, "position must round-trip");
        assert_eq!(entry.context_type, None, "context type must round-trip");
        assert_eq!(entry.context_id, None, "context id must round-trip");
        assert_eq!(
            entry.added_at, "2024-01-01T00:00:00Z",
            "added_at must round-trip"
        );
        let cloned = entry.clone();
        assert_eq!(cloned.id, 1, "clone must preserve id");
        assert!(
            format!("{entry:?}").contains('2'),
            "debug must include track id"
        );
    }
}

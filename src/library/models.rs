//! Data models for the music library database.
//!
//! This module defines the core data structures used throughout the library system,
//! including Album, Artist, and Track models with proper serde serialization.

use {
    serde::{Deserialize, Serialize},
    sqlx::FromRow,
};

/// Represents a musical artist in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow, Default)]
pub struct Artist {
    /// Unique database ID.
    pub id: i64,
    /// Artist name.
    pub name: String,
    /// Number of albums by this artist.
    pub album_count: i64,
    /// Timestamp when the artist was first added to the library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Timestamp when the artist was last updated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Represents a musical album in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow, Default)]
pub struct Album {
    /// Unique database ID.
    pub id: i64,
    /// ID of the associated artist.
    pub artist_id: i64,
    /// Album title.
    pub title: String,
    /// Release year (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i64>,
    /// Genre (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// Audio format (e.g., "FLAC", "MP3").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Bits per sample for the album's audio files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<i64>,
    /// Sample rate in Hz for the album's audio files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,
    /// Whether this is a compilation album.
    pub compilation: bool,
    /// File system path to the album directory.
    pub path: String,
    /// DR (Dynamic Range) value (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dr_value: Option<String>,
    /// Path to album artwork file (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_path: Option<String>,
    /// Timestamp when the album was first added to the library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Timestamp when the album was last updated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Represents a track in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct Track {
    /// Unique database ID.
    pub id: i64,
    /// ID of the associated album.
    pub album_id: i64,
    /// Track title.
    pub title: String,
    /// Track number within the album.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i64>,
    /// Disc number (defaults to 1).
    pub disc_number: i64,
    /// Duration in milliseconds.
    pub duration_ms: i64,
    /// File system path to the audio file.
    pub path: String,
    /// File size in bytes.
    pub file_size: i64,
    /// Audio format (e.g., "FLAC", "MP3").
    pub format: String,
    /// Audio codec (e.g., "FLAC", "MP3", "PCM S24").
    pub codec: String,
    /// Sample rate in Hz.
    pub sample_rate: i64,
    /// Bits per sample.
    pub bits_per_sample: i64,
    /// Number of audio channels.
    pub channels: i64,
    /// Whether the format is lossless.
    pub is_lossless: bool,
    /// Whether the format is high-resolution.
    pub is_high_resolution: bool,
    /// Timestamp when the track was first added to the library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Timestamp when the track was last updated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Search results containing albums and artists that match a query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matching albums.
    pub albums: Vec<Album>,
    /// Matching artists.
    pub artists: Vec<Artist>,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            id: 0,
            album_id: 0,
            title: String::new(),
            track_number: None,
            disc_number: 1,
            duration_ms: 0,
            path: String::new(),
            file_size: 0,
            format: String::new(),
            codec: String::new(),
            sample_rate: 44100,
            bits_per_sample: 16,
            channels: 2,
            is_lossless: false,
            is_high_resolution: false,
            created_at: None,
            updated_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use crate::library::models::{Album, Artist, Track};

    #[test]
    fn test_artist_serialization() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 5,
            created_at: Some("2023-01-01 00:00:00".to_string()),
            updated_at: Some("2023-01-02 00:00:00".to_string()),
        };

        let serialized = to_string(&artist).unwrap();
        let deserialized: Artist = from_str(&serialized).unwrap();
        assert_eq!(artist, deserialized);
    }

    #[test]
    fn test_album_serialization() {
        let album = Album {
            id: 1,
            artist_id: 1,
            title: "Test Album".to_string(),
            year: Some(2023),
            genre: Some("Classical".to_string()),
            format: Some("FLAC".to_string()),
            bits_per_sample: Some(24),
            sample_rate: Some(96000),
            compilation: false,
            path: "/path/to/album".to_string(),
            dr_value: Some("DR12".to_string()),
            artwork_path: Some("/path/to/album/folder.jpg".to_string()),
            created_at: Some("2023-01-01 00:00:00".to_string()),
            updated_at: Some("2023-01-02 00:00:00".to_string()),
        };

        let serialized = to_string(&album).unwrap();
        let deserialized: Album = from_str(&serialized).unwrap();
        assert_eq!(album, deserialized);
    }

    #[test]
    fn test_track_serialization() {
        let track = Track {
            id: 1,
            album_id: 1,
            title: "Test Track".to_string(),
            track_number: Some(1),
            disc_number: 1,
            duration_ms: 300_000,
            path: "/path/to/track.flac".to_string(),
            file_size: 1024,
            format: "FLAC".to_string(),
            codec: "FLAC".to_string(),
            sample_rate: 96000,
            bits_per_sample: 24,
            channels: 2,
            is_lossless: true,
            is_high_resolution: true,
            created_at: Some("2023-01-01 00:00:00".to_string()),
            updated_at: Some("2023-01-02 00:00:00".to_string()),
        };

        let serialized = to_string(&track).unwrap();
        let deserialized: Track = from_str(&serialized).unwrap();
        assert_eq!(track, deserialized);
    }

    #[test]
    fn test_default_implementations() {
        let track = Track::default();
        assert_eq!(track.disc_number, 1);
        assert_eq!(track.sample_rate, 44100);

        let album = Album::default();
        assert!(!album.compilation);

        let artist = Artist::default();
        assert_eq!(artist.name, "");
        assert_eq!(artist.album_count, 0);
    }
}

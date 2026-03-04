//! Data models for the music library database.
//!
//! This module defines the core data structures used throughout the library system,
//! including Album, Artist, and Track models with proper serde serialization.

use {
    serde::{Deserialize, Serialize},
    sqlx::FromRow,
    tracing::warn,
};

use crate::audio::constants::{DEFAULT_BIT_DEPTH, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};

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
    /// Number of tracks in the album (computed from JOIN).
    #[sqlx(default)]
    pub track_count: i64,
    /// Number of audio channels (representative, e.g., MAX).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub channels: Option<i64>,
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

impl Album {
    /// Extracts the numeric DR value for sorting.
    #[must_use]
    pub fn dr_value_numeric(&self) -> Option<i64> {
        let dr_str = self.dr_value.as_ref()?;
        let numeric_part = dr_str.strip_prefix("DR")?;
        numeric_part.parse().map_or_else(
            |_| {
                warn!(
                    album_id = self.id,
                    dr_value = %dr_str,
                    "Failed to parse DR value, expected format DR<number>"
                );
                None
            },
            Some,
        )
    }
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
            sample_rate: i64::from(DEFAULT_SAMPLE_RATE),
            bits_per_sample: i64::from(DEFAULT_BIT_DEPTH),
            channels: i64::from(DEFAULT_CHANNELS),
            is_lossless: false,
            is_high_resolution: false,
            created_at: None,
            updated_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, bail},
        serde_json::{from_str, to_string},
    };

    use crate::{
        audio::constants::DEFAULT_SAMPLE_RATE,
        library::models::{Album, Artist, Track},
    };

    #[test]
    fn test_artist_serialization() -> Result<()> {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 5,
            created_at: Some("2023-01-01 00:00:00".to_string()),
            updated_at: Some("2023-01-02 00:00:00".to_string()),
        };

        let serialized = to_string(&artist)?;
        let deserialized: Artist = from_str(&serialized)?;
        if artist != deserialized {
            bail!(
                "Artist serialization roundtrip failed: expected {artist:?}, got {deserialized:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_album_serialization() -> Result<()> {
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
            track_count: 12,
            channels: Some(2),
        };

        let serialized = to_string(&album)?;
        let deserialized: Album = from_str(&serialized)?;
        if album != deserialized {
            bail!("Album serialization roundtrip failed: expected {album:?}, got {deserialized:?}");
        }
        Ok(())
    }

    #[test]
    fn test_track_serialization() -> Result<()> {
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

        let serialized = to_string(&track)?;
        let deserialized: Track = from_str(&serialized)?;
        if track != deserialized {
            bail!("Track serialization roundtrip failed: expected {track:?}, got {deserialized:?}");
        }
        Ok(())
    }

    #[test]
    fn test_default_implementations() -> Result<()> {
        let track = Track::default();
        if track.disc_number != 1 {
            bail!("Expected disc_number to be 1");
        }
        if track.sample_rate != i64::from(DEFAULT_SAMPLE_RATE) {
            bail!("Expected sample_rate to be DEFAULT_SAMPLE_RATE");
        }

        let album = Album::default();
        if album.compilation {
            bail!("Expected compilation to be false");
        }

        let artist = Artist::default();
        if !artist.name.is_empty() {
            bail!("Expected name to be empty");
        }
        if artist.album_count != 0 {
            bail!("Expected album_count to be 0");
        }
        Ok(())
    }
}

//! Audio file metadata extraction using the `lofty` crate.
//!
//! This module provides functionality to extract both standard metadata
//! (artist, album, title, etc.) and technical Hi-Fi metadata (format,
//! bit depth, sample rate, duration) from audio files.

use std::{fs::metadata, io::Error as StdError, path::Path};

use {
    anyhow::Context,
    lofty::{
        error::{ErrorKind::Io, LoftyError},
        picture::PictureType::{CoverBack, CoverFront, Leaflet},
        prelude::{AudioFile, ItemKey::AlbumArtist, TaggedFileExt},
        probe::Probe,
        tag::Accessor,
    },
    serde::{Deserialize, Serialize},
    thiserror::Error,
};

use crate::audio::format_detector::detect_audio_format;

/// Error type for metadata extraction operations.
#[derive(Error, Debug)]
pub enum MetadataError {
    /// Failed to read or parse the audio file.
    #[error("Failed to read audio file: {0}")]
    ReadError(#[from] LoftyError),
    /// The file format is not supported.
    #[error("Unsupported file format")]
    UnsupportedFormat,
    /// Missing required metadata fields.
    #[error("Missing required metadata field: {field}")]
    MissingField { field: String },
}

/// Standard audio metadata extracted from tags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardMetadata {
    /// Track title.
    pub title: Option<String>,
    /// Track artist.
    pub artist: Option<String>,
    /// Album name.
    pub album: Option<String>,
    /// Album artist.
    pub album_artist: Option<String>,
    /// Track number.
    pub track_number: Option<u32>,
    /// Total tracks in album.
    pub total_tracks: Option<u32>,
    /// Disc number.
    pub disc_number: Option<u32>,
    /// Total discs in collection.
    pub total_discs: Option<u32>,
    /// Release year.
    pub year: Option<u32>,
    /// Genre.
    pub genre: Option<String>,
    /// Comment.
    pub comment: Option<String>,
}

/// Technical Hi-Fi metadata about the audio file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TechnicalMetadata {
    /// Audio format (e.g., "FLAC", "MP3", "WAV").
    pub format: String,
    /// Audio codec (e.g., "FLAC", "MP3", "PCM S24").
    pub codec: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Bits per sample.
    pub bits_per_sample: u32,
    /// Number of audio channels.
    pub channels: u32,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// File size in bytes.
    pub file_size: u64,
    /// Whether the format is lossless.
    pub is_lossless: bool,
    /// Whether the format is high-resolution.
    pub is_high_resolution: bool,
}

/// Combined metadata containing both standard and technical information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMetadata {
    /// Standard tag-based metadata.
    pub standard: StandardMetadata,
    /// Technical audio properties.
    pub technical: TechnicalMetadata,
    /// Embedded artwork data (if available).
    pub artwork: Option<Vec<u8>>,
}

/// Extracts metadata from an audio file.
///
/// # Arguments
///
/// * `path` - Path to the audio file.
///
/// # Returns
///
/// A `Result` containing the extracted `TrackMetadata` or a `MetadataError`.
///
/// # Examples
///
/// ```no_run
/// use oxhidifi::audio::metadata::TagReader;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let metadata = TagReader::read_metadata("/path/to/song.flac")?;
///     println!("Title: {:?}", metadata.standard.title);
///     println!("Sample rate: {} Hz", metadata.technical.sample_rate);
///     Ok(())
/// }
/// ```
pub struct TagReader;

impl TagReader {
    /// Reads and extracts complete metadata from an audio file.
    ///
    /// This method performs a comprehensive analysis of the audio file,
    /// extracting both standard tag information and technical audio properties.
    ///
    /// # Errors
    ///
    /// Returns `MetadataError` if:
    /// - The file cannot be read or parsed
    /// - The file format is unsupported
    /// - Required technical metadata cannot be determined
    pub fn read_metadata<P: AsRef<Path>>(path: P) -> Result<TrackMetadata, MetadataError> {
        let path = path.as_ref();

        // Get file size
        let file_size = metadata(path)
            .context("Failed to get file metadata")
            .map_err(|e| {
                let io_error = StdError::other(e.to_string());
                MetadataError::ReadError(LoftyError::new(Io(io_error)))
            })?
            .len();

        // Probe the audio file
        let probe = Probe::open(path).map_err(MetadataError::ReadError)?;

        let tagged_file = probe.read().map_err(MetadataError::ReadError)?;

        let primary_tag = tagged_file.primary_tag();
        let properties = tagged_file.properties();

        // Extract standard metadata
        let standard = StandardMetadata {
            title: primary_tag.and_then(|tag| tag.title().map(|s| s.to_string())),
            artist: primary_tag.and_then(|tag| tag.artist().map(|s| s.to_string())),
            album: primary_tag.and_then(|tag| tag.album().map(|s| s.to_string())),
            album_artist: primary_tag.and_then(|tag| {
                tag.get_string(&AlbumArtist)
                    .map(|s| s.to_string())
                    .or_else(|| tag.artist().map(|s| s.to_string()))
            }),
            track_number: primary_tag.and_then(|tag| tag.track()),
            total_tracks: primary_tag.and_then(|tag| tag.track_total()),
            disc_number: primary_tag.and_then(|tag| tag.disk()),
            total_discs: primary_tag.and_then(|tag| tag.disk_total()),
            year: primary_tag.and_then(|tag| tag.year()),
            genre: primary_tag.and_then(|tag| tag.genre().map(|s| s.to_string())),
            comment: primary_tag.and_then(|tag| tag.comment().map(|s| s.to_string())),
        };

        // Extract technical metadata using symphonia's full capabilities
        let format_info = detect_audio_format(path).map_err(|e| {
            MetadataError::ReadError(LoftyError::new(Io(StdError::other(e.to_string()))))
        })?;

        let technical = TechnicalMetadata {
            format: format_info.format,
            codec: format_info.codec,
            sample_rate: properties.sample_rate().unwrap_or(format_info.sample_rate),
            bits_per_sample: properties
                .bit_depth()
                .map(|b| b as u32)
                .unwrap_or(format_info.bits_per_sample),
            channels: properties
                .channels()
                .map(|c| c as u32)
                .unwrap_or(format_info.channels),
            duration_ms: properties.duration().as_millis() as u64,
            file_size,
            is_lossless: format_info.is_lossless,
            is_high_resolution: format_info.is_high_resolution,
        };

        // Extract embedded artwork (prefer front cover)
        let artwork = primary_tag.and_then(|tag| {
            // Try to get front cover first
            tag.get_picture_type(CoverFront)
                .or_else(|| {
                    // Fall back to any cover if front cover not available
                    tag.pictures()
                        .iter()
                        .find(|pic| matches!(pic.pic_type(), CoverFront | CoverBack | Leaflet))
                })
                .map(|pic| pic.data().to_vec())
        });

        Ok(TrackMetadata {
            standard,
            technical,
            artwork,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use crate::audio::metadata::{
        MetadataError, StandardMetadata, TechnicalMetadata, TrackMetadata,
    };

    #[test]
    fn test_metadata_error_display() {
        let error = MetadataError::UnsupportedFormat;
        assert_eq!(error.to_string(), "Unsupported file format");

        let missing_field_error = MetadataError::MissingField {
            field: "title".to_string(),
        };
        assert_eq!(
            missing_field_error.to_string(),
            "Missing required metadata field: title"
        );
    }

    #[test]
    fn test_standard_metadata_serialization() {
        let metadata = StandardMetadata {
            title: Some("Test Title".to_string()),
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            album_artist: None,
            track_number: Some(1),
            total_tracks: Some(10),
            disc_number: Some(1),
            total_discs: Some(1),
            year: Some(2023),
            genre: Some("Classical".to_string()),
            comment: Some("Test comment".to_string()),
        };

        let serialized = to_string(&metadata).unwrap();
        let deserialized: StandardMetadata = from_str(&serialized).unwrap();
        assert_eq!(metadata, deserialized);
    }

    #[test]
    fn test_technical_metadata_serialization() {
        let metadata = TechnicalMetadata {
            format: "FLAC".to_string(),
            codec: "FLAC".to_string(),
            sample_rate: 96000,
            bits_per_sample: 24,
            channels: 2,
            duration_ms: 300000,
            file_size: 1024,
            is_lossless: true,
            is_high_resolution: true,
        };

        let serialized = to_string(&metadata).unwrap();
        let deserialized: TechnicalMetadata = from_str(&serialized).unwrap();
        assert_eq!(metadata, deserialized);
    }

    #[test]
    fn test_track_metadata_serialization() {
        let metadata = TrackMetadata {
            standard: StandardMetadata {
                title: Some("Test Title".to_string()),
                artist: Some("Test Artist".to_string()),
                album: Some("Test Album".to_string()),
                album_artist: None,
                track_number: Some(1),
                total_tracks: Some(10),
                disc_number: Some(1),
                total_discs: Some(1),
                year: Some(2023),
                genre: Some("Classical".to_string()),
                comment: Some("Test comment".to_string()),
            },
            technical: TechnicalMetadata {
                format: "FLAC".to_string(),
                codec: "FLAC".to_string(),
                sample_rate: 96000,
                bits_per_sample: 24,
                channels: 2,
                duration_ms: 300000,
                file_size: 1024,
                is_lossless: true,
                is_high_resolution: true,
            },
            artwork: None,
        };

        let serialized = to_string(&metadata).unwrap();
        let deserialized: TrackMetadata = from_str(&serialized).unwrap();
        assert_eq!(metadata, deserialized);
    }
}

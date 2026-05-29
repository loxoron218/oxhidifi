//! Metadata extraction from audio files using the `lofty` crate.

use std::{fs::metadata, path::Path};

use {
    lofty::{
        error::LoftyError,
        file::{
            AudioFile,
            FileType::{self, Aiff, Flac, Mp4, Mpeg, Opus, Vorbis, Wav},
            TaggedFile, TaggedFileExt,
        },
        prelude::Accessor,
        read_from_path,
        tag::ItemKey::RecordingDate,
    },
    thiserror::Error,
};

/// Extracted metadata from an audio file.
#[derive(Debug, Clone)]
pub struct AudioMetadata {
    /// Track title.
    pub title: Option<String>,
    /// Artist name.
    pub artist: Option<String>,
    /// Album title.
    pub album: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Track number within album/disc.
    pub track_number: Option<i32>,
    /// Disc number.
    pub disc_number: Option<i32>,
    /// Duration in seconds.
    pub duration: f64,
    /// Sample rate in Hz.
    pub sample_rate: i32,
    /// Bit depth (None for lossy formats).
    pub bit_depth: Option<i32>,
    /// Number of audio channels.
    pub channels: i32,
    /// Codec identifier.
    pub codec: String,
    /// Whether format is lossless.
    pub lossless: bool,
    /// Average bitrate in kbps.
    pub bitrate: Option<i32>,
    /// File size in bytes.
    pub file_size: i64,
}

/// Errors occurring during metadata extraction.
#[derive(Debug, Error)]
pub enum MetadataError {
    /// Failed to read or parse the audio file.
    #[error("Failed to read audio file: {0}")]
    ReadError(#[from] LoftyError),
    /// File does not exist or is not a regular file.
    #[error("File not found or inaccessible: {0}")]
    FileNotFound(String),
    /// Duration is zero or negative (corrupt file).
    #[error("Invalid duration: {0}s")]
    InvalidDuration(f64),
    /// Failed to parse a tag value.
    #[error("Failed to parse tag value: {0}")]
    ParseError(String),
}

/// Extract metadata from an audio file at the given path.
///
/// # Arguments
///
/// * `path` - Path to the audio file
///
/// # Returns
///
/// A `Result` containing the extracted metadata or an error.
///
/// # Errors
///
/// Returns [`MetadataError`] if the file cannot be read, parsed, or has invalid properties.
pub fn extract_metadata(path: &Path) -> Result<AudioMetadata, MetadataError> {
    let tagged_file = read_from_path(path)?;
    let props = tagged_file.properties();
    let file_type = tagged_file.file_type();

    let title = extract_title(&tagged_file, path);
    let artist = extract_artist(&tagged_file);
    let album = extract_album(&tagged_file);
    let year = extract_year(&tagged_file)?;
    let genre = extract_genre(&tagged_file);
    let track_number = extract_track_number(&tagged_file);
    let disc_number = extract_disc_number(&tagged_file);

    let duration = props.duration().as_secs_f64();
    if duration <= 0.0 {
        return Err(MetadataError::InvalidDuration(duration));
    }

    let sample_rate = i32::try_from(props.sample_rate().unwrap_or(0)).unwrap_or(0);

    let bit_depth = props.bit_depth().map(i32::from);

    let channels = i32::from(props.channels().unwrap_or(0));

    let codec = codec_name(file_type);

    let lossless = matches!(file_type, Flac | Wav | Aiff);

    let bitrate = props.audio_bitrate().map(u32::cast_signed);

    let file_size = metadata(path).map_or(0, |m| m.len().cast_signed());

    Ok(AudioMetadata {
        title,
        artist,
        album,
        year,
        genre,
        track_number,
        disc_number,
        duration,
        sample_rate,
        bit_depth,
        channels,
        codec,
        lossless,
        bitrate,
        file_size,
    })
}

/// Extract the title from tags, falling back to filename stem.
fn extract_title(tagged_file: &TaggedFile, path: &Path) -> Option<String> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    tag.as_ref()
        .and_then(|t| t.title().map(String::from))
        .or_else(|| path.file_stem().and_then(|s| s.to_str()).map(String::from))
}

/// Extract the artist name from tags.
fn extract_artist(tagged_file: &TaggedFile) -> Option<String> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;

    tag.artist().map(String::from)
}

/// Extract the album title from tags.
fn extract_album(tagged_file: &TaggedFile) -> Option<String> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;

    tag.album().map(String::from)
}

/// Extract the release year from tags.
///
/// # Errors
///
/// Returns [`MetadataError::ParseError`] if the year tag value cannot be parsed as an integer.
fn extract_year(tagged_file: &TaggedFile) -> Result<Option<i32>, MetadataError> {
    let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    else {
        return Ok(None);
    };

    let Some(s) = tag.get_string(RecordingDate) else {
        return Ok(None);
    };

    s.parse::<i32>()
        .map(Some)
        .map_err(|e| MetadataError::ParseError(e.to_string()))
}

/// Extract the genre from tags.
fn extract_genre(tagged_file: &TaggedFile) -> Option<String> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;

    tag.genre().map(String::from)
}

/// Extract the track number from tags.
fn extract_track_number(tagged_file: &TaggedFile) -> Option<i32> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;

    tag.track().map(u32::cast_signed)
}

/// Extract the disc number from tags.
fn extract_disc_number(tagged_file: &TaggedFile) -> Option<i32> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?;

    tag.disk().map(u32::cast_signed)
}

/// Get a human-readable codec name from the file type.
fn codec_name(file_type: FileType) -> String {
    match file_type {
        Flac => "flac".to_string(),
        Mpeg => "mp3".to_string(),
        Mp4 => "aac".to_string(),
        Vorbis => "ogg".to_string(),
        Opus => "opus".to_string(),
        Wav => "wav".to_string(),
        Aiff => "aiff".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Compute a metadata fingerprint for duplicate detection.
///
/// Returns a tuple of (artist, album, title, `track_number`) suitable for comparison.
#[must_use]
pub fn metadata_fingerprint(meta: &AudioMetadata) -> (String, String, String, Option<i32>) {
    let artist = meta
        .artist
        .as_deref()
        .unwrap_or("Unknown Artist")
        .to_lowercase();
    let album = meta
        .album
        .as_deref()
        .unwrap_or("Unknown Album")
        .to_lowercase();
    let title = meta
        .title
        .as_deref()
        .unwrap_or("Unknown Track")
        .to_lowercase();
    (artist, album, title, meta.track_number)
}

#[cfg(test)]
pub mod tests {
    use std::path::Path;

    use {
        anyhow::{Result, bail},
        lofty::file::FileType::{Aiff, Flac, Mp4, Mpeg, Opus, Vorbis, Wav},
    };

    use crate::library::metadata::{
        AudioMetadata, codec_name, extract_metadata, metadata_fingerprint,
    };

    #[must_use]
    pub fn test_metadata() -> AudioMetadata {
        AudioMetadata {
            title: Some("My Track".to_string()),
            artist: Some("Some Artist".to_string()),
            album: Some("Some Album".to_string()),
            year: Some(2024),
            genre: Some("Rock".to_string()),
            track_number: Some(3),
            disc_number: Some(1),
            duration: 240.0,
            sample_rate: 44100,
            bit_depth: Some(16),
            channels: 2,
            codec: "flac".to_string(),
            lossless: true,
            bitrate: None,
            file_size: 1024,
        }
    }

    #[must_use]
    pub fn test_metadata_defaults() -> AudioMetadata {
        AudioMetadata {
            title: None,
            artist: None,
            album: None,
            year: None,
            genre: None,
            track_number: None,
            disc_number: None,
            duration: 120.0,
            sample_rate: 44100,
            bit_depth: None,
            channels: 2,
            codec: "mp3".to_string(),
            lossless: false,
            bitrate: Some(320),
            file_size: 2048,
        }
    }

    #[test]
    fn extract_metadata_missing_file() -> Result<()> {
        let result = extract_metadata(Path::new("/nonexistent/file.flac"));
        if result.is_ok() {
            bail!("expected error for nonexistent file");
        }
        Ok(())
    }

    #[test]
    fn metadata_fingerprint_normalizes() {
        let meta = test_metadata();
        let (artist, album, title, track) = metadata_fingerprint(&meta);
        assert_eq!(artist, "some artist");
        assert_eq!(album, "some album");
        assert_eq!(title, "my track");
        assert_eq!(track, Some(3));
    }

    #[test]
    fn metadata_fingerprint_defaults() {
        let meta = test_metadata_defaults();
        let (artist, album, title, track) = metadata_fingerprint(&meta);
        assert_eq!(artist, "unknown artist");
        assert_eq!(album, "unknown album");
        assert_eq!(title, "unknown track");
        assert_eq!(track, None);
    }

    #[test]
    fn codec_name_variants() {
        assert_eq!(codec_name(Flac), "flac");
        assert_eq!(codec_name(Mpeg), "mp3");
        assert_eq!(codec_name(Mp4), "aac");
        assert_eq!(codec_name(Vorbis), "ogg");
        assert_eq!(codec_name(Opus), "opus");
        assert_eq!(codec_name(Wav), "wav");
        assert_eq!(codec_name(Aiff), "aiff");
    }
}

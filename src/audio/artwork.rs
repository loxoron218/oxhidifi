//! Artwork extraction and management utilities.
//!
//! This module provides functionality to extract embedded artwork from audio files.

use std::{fs::write, io::Error, path::Path};

use {
    lofty::picture::MimeType::{self, Bmp, Gif, Jpeg, Png},
    thiserror::Error,
};

use crate::audio::metadata::{MetadataError, TagReader};

/// Error type for artwork extraction operations.
#[derive(Error, Debug)]
pub enum ArtworkError {
    /// Failed to write artwork file.
    #[error("Failed to write artwork file: {0}")]
    WriteError(#[from] Error),
    /// Failed to extract embedded artwork.
    #[error("Failed to extract embedded artwork: {0}")]
    ExtractionError(#[from] MetadataError),
    /// No artwork found.
    #[error("No artwork found")]
    NotFound,
}

/// Extracts embedded artwork from an audio file.
///
/// # Arguments
///
/// * `audio_path` - Path to the audio file.
///
/// # Returns
///
/// A `Result` containing a tuple of `(Vec<u8>, Option<MimeType>)` or an `ArtworkError`.
///
/// # Errors
///
/// Returns `ArtworkError::ExtractionError` if metadata extraction fails.
/// Returns `ArtworkError::NotFound` if no embedded artwork is found.
pub fn extract_artwork<P: AsRef<Path>>(
    audio_path: P,
) -> Result<(Vec<u8>, Option<MimeType>), ArtworkError> {
    let audio_path = audio_path.as_ref();

    let metadata = TagReader::read_metadata(audio_path)?;

    let artwork_data = metadata.artwork.ok_or(ArtworkError::NotFound)?;

    let mime_type = detect_mime_type(&artwork_data);

    Ok((artwork_data, mime_type))
}

/// Detects MIME type from binary data.
///
/// # Arguments
///
/// * `data` - Binary artwork data.
///
/// # Returns
///
/// An `Option<MimeType>` representing the detected MIME type.
#[must_use]
pub fn detect_mime_type(data: &[u8]) -> Option<MimeType> {
    if data.len() < 4 {
        return None;
    }

    // Check JPEG magic bytes
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some(Jpeg);
    }

    // Check PNG magic bytes
    if data[0] == 0x89 && data[1] == b'P' && data[2] == b'N' && data[3] == b'G' {
        return Some(Png);
    }

    // Check GIF magic bytes
    if data[0] == b'G' && data[1] == b'I' && data[2] == b'F' {
        return Some(Gif);
    }

    // Check BMP magic bytes
    if data[0] == b'B' && data[1] == b'M' {
        return Some(Bmp);
    }

    None
}

/// Saves embedded artwork to a file.
///
/// # Arguments
///
/// * `artwork_data` - Embedded artwork binary data.
/// * `target_path` - Path where the artwork should be saved.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `ArtworkError::WriteError` if the file cannot be written.
pub fn save_embedded_artwork<P: AsRef<Path>>(
    artwork_data: &[u8],
    target_path: P,
) -> Result<(), ArtworkError> {
    write(target_path, artwork_data).map_err(ArtworkError::WriteError)
}

#[cfg(test)]
mod tests {
    use lofty::picture::MimeType::{Jpeg, Png};

    use crate::audio::artwork::detect_mime_type;

    #[test]
    fn test_detect_mime_type() {
        // Test JPEG
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_mime_type(&jpeg_data), Some(Jpeg));

        // Test PNG
        let png_data = vec![0x89, b'P', b'N', b'G'];
        assert_eq!(detect_mime_type(&png_data), Some(Png));

        // Test unknown
        let unknown_data = vec![0x00, 0x01, 0x02, 0x03];
        assert_eq!(detect_mime_type(&unknown_data), None);
    }
}

//! Artwork extraction and management utilities.
//!
//! This module provides functionality to extract embedded artwork from audio files
//! and detect external artwork files in album directories.

use std::{
    fs::{read_dir, write},
    io::Error,
    path::{Path, PathBuf},
};

use {
    lofty::picture::MimeType::{self, Bmp, Gif, Jpeg, Png},
    thiserror::Error,
};

use crate::audio::metadata::{MetadataError, TagReader};

/// Error type for artwork extraction operations.
#[derive(Error, Debug)]
pub enum ArtworkError {
    /// Failed to read artwork file.
    #[error("Failed to read artwork file: {0}")]
    ReadError(#[from] Error),
    /// Failed to extract embedded artwork.
    #[error("Failed to extract embedded artwork: {0}")]
    ExtractionError(#[from] MetadataError),
    /// No artwork found.
    #[error("No artwork found")]
    NotFound,
}

/// Artwork source types.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtworkSource {
    /// Embedded artwork from audio file tags.
    Embedded(Vec<u8>, Option<MimeType>),
    /// External artwork file.
    External(PathBuf),
}

/// Extracts artwork from an audio file or its directory.
///
/// This function attempts to extract artwork in the following order:
/// 1. Embedded artwork from the audio file (front cover preferred)
/// 2. Common external artwork files in the same directory as the audio file
///
/// # Arguments
///
/// * `audio_path` - Path to the audio file.
///
/// # Returns
///
/// A `Result` containing the `ArtworkSource` or an `ArtworkError`.
///
/// # Errors
///
/// Returns `ArtworkError::ExtractionError` if metadata extraction fails.
/// Returns `ArtworkError::ReadError` if the directory cannot be read.
/// Returns `ArtworkError::NotFound` if no artwork is found.
pub fn extract_artwork<P: AsRef<Path>>(audio_path: P) -> Result<ArtworkSource, ArtworkError> {
    let audio_path = audio_path.as_ref();
    let parent_dir = audio_path.parent().ok_or(ArtworkError::NotFound)?;

    // First, try to extract embedded artwork
    if let Ok(metadata) = TagReader::read_metadata(audio_path)
        && let Some(artwork_data) = metadata.artwork
    {
        // Try to determine MIME type from the data
        let mime_type = detect_mime_type(&artwork_data);
        return Ok(ArtworkSource::Embedded(artwork_data, mime_type));
    }

    // If no embedded artwork, look for external files
    if let Some(external_path) = find_external_artwork(parent_dir)? {
        return Ok(ArtworkSource::External(external_path));
    }

    Err(ArtworkError::NotFound)
}

/// Finds external artwork files in a directory.
///
/// Searches for common artwork file names in the specified directory.
///
/// # Arguments
///
/// * `dir` - Directory to search for artwork files.
///
/// # Returns
///
/// A `Result` containing an `Option<PathBuf>` with the path to the artwork file,
/// or an `ArtworkError` if the directory cannot be read.
///
/// # Errors
///
/// Returns `ArtworkError::ReadError` if the directory cannot be read.
pub fn find_external_artwork(dir: &Path) -> Result<Option<PathBuf>, ArtworkError> {
    let entries = read_dir(dir).map_err(ArtworkError::ReadError)?;

    // Common artwork file names (in order of preference)
    let artwork_names = [
        "folder.jpg",
        "folder.jpeg",
        "cover.jpg",
        "cover.jpeg",
        "album.jpg",
        "album.jpeg",
        "front.jpg",
        "front.jpeg",
        "artwork.jpg",
        "artwork.jpeg",
        "folder.png",
        "cover.png",
        "album.png",
        "front.png",
        "artwork.png",
    ];

    // First pass: exact matches
    for entry in entries {
        let entry = entry.map_err(ArtworkError::ReadError)?;
        let path = entry.path();

        if path.is_file()
            && let Some(filename) = path.file_name().and_then(|n| n.to_str())
            && artwork_names
                .iter()
                .any(|&name| name.eq_ignore_ascii_case(filename))
        {
            return Ok(Some(path));
        }
    }

    // Second pass: any image file (fallback)
    let entries = read_dir(dir).map_err(ArtworkError::ReadError)?;
    for entry in entries {
        let entry = entry.map_err(ArtworkError::ReadError)?;
        let path = entry.path();

        if path.is_file()
            && let Some(extension) = path.extension().and_then(|ext| ext.to_str())
            && matches!(
                extension.to_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp"
            )
        {
            return Ok(Some(path));
        }
    }

    Ok(None)
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
/// Returns `ArtworkError::ReadError` if the file cannot be written.
pub fn save_embedded_artwork<P: AsRef<Path>>(
    artwork_data: &[u8],
    target_path: P,
) -> Result<(), ArtworkError> {
    write(target_path, artwork_data).map_err(ArtworkError::ReadError)
}

#[cfg(test)]
mod tests {
    use super::*;

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

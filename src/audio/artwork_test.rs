//! Comprehensive tests for artwork extraction functionality.
//!
//! This module contains integration tests that verify artwork extraction
//! works correctly with various audio formats and scenarios.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use {
        anyhow::{Result, anyhow, bail},
        lofty::picture::MimeType::{Jpeg, Png},
    };

    use crate::audio::{
        artwork::{detect_mime_type, extract_artwork},
        metadata::TagReader,
    };

    /// Test embedded artwork extraction from FLAC files.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_embedded_artwork_flac() -> Result<()> {
        let test_file = Path::new("testdata/flac_with_artwork.flac");
        if test_file.exists() {
            let (data, mime_type) = extract_artwork(test_file)?;
            if data.is_empty() {
                bail!("Expected non-empty data, got empty");
            }
            if mime_type.is_none() {
                bail!("Expected Some(mime_type), got None");
            }
        }
        Ok(())
    }

    /// Test embedded artwork extraction from MP3 files.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_embedded_artwork_mp3() -> Result<()> {
        let test_file = Path::new("testdata/mp3_with_artwork.mp3");
        if test_file.exists() {
            let (data, mime_type) = extract_artwork(test_file)?;
            if data.is_empty() {
                bail!("Expected non-empty data, got empty");
            }
            if mime_type.is_none() {
                bail!("Expected Some(mime_type), got None");
            }
        }
        Ok(())
    }

    /// Test artwork extraction fallback behavior.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_artwork_extraction_fallback() -> Result<()> {
        let test_file = Path::new("testdata/audio_without_artwork.flac");
        if test_file.exists() {
            let result = extract_artwork(test_file);

            // Should fail gracefully if no artwork is found
            let Err(_) = result else {
                bail!("Expected error, got Ok");
            };
        }
        Ok(())
    }

    /// Test MIME type detection.
    #[test]
    fn test_mime_type_detection() -> Result<()> {
        // Test JPEG detection
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        let mime_type = detect_mime_type(&jpeg_data);
        if mime_type != Some(Jpeg) {
            bail!("Expected Some(Jpeg), got {mime_type:?}");
        }

        // Test PNG detection
        let png_data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mime_type = detect_mime_type(&png_data);
        if mime_type != Some(Png) {
            bail!("Expected Some(Png), got {mime_type:?}");
        }

        // Test unknown format
        let unknown_data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let mime_type = detect_mime_type(&unknown_data);
        if mime_type.is_some() {
            bail!("Expected None, got {mime_type:?}");
        }
        Ok(())
    }

    /// Test metadata extraction with artwork.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_metadata_extraction_with_artwork() -> Result<()> {
        let test_file = Path::new("testdata/flac_with_artwork.flac");
        if test_file.exists() {
            let metadata = TagReader::read_metadata(test_file)?;
            if metadata.artwork.is_none() {
                bail!("Expected Some(artwork), got None");
            }
            let artwork = metadata
                .artwork
                .as_ref()
                .ok_or_else(|| anyhow!("no artwork"))?;
            if artwork.is_empty() {
                bail!("Expected non-empty artwork, got empty");
            }
        }
        Ok(())
    }
}

//! Comprehensive tests for artwork extraction functionality.
//!
//! This module contains integration tests that verify artwork extraction
//! works correctly with various audio formats and scenarios.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lofty::picture::MimeType::{Jpeg, Png};

    use crate::audio::{
        artwork::{
            ArtworkSource::{Embedded, External},
            detect_mime_type, extract_artwork,
        },
        metadata::TagReader,
    };

    /// Test embedded artwork extraction from FLAC files.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_embedded_artwork_flac() {
        let test_file = Path::new("testdata/flac_with_artwork.flac");
        if test_file.exists() {
            let result = extract_artwork(test_file);
            assert!(result.is_ok());
            match result.unwrap() {
                Embedded(data, mime_type) => {
                    assert!(!data.is_empty());
                    assert!(mime_type.is_some());
                }
                _ => panic!("Expected embedded artwork"),
            }
        }
    }

    /// Test embedded artwork extraction from MP3 files.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_embedded_artwork_mp3() {
        let test_file = Path::new("testdata/mp3_with_artwork.mp3");
        if test_file.exists() {
            let result = extract_artwork(test_file);
            assert!(result.is_ok());
            match result.unwrap() {
                Embedded(data, mime_type) => {
                    assert!(!data.is_empty());
                    assert!(mime_type.is_some());
                }
                _ => panic!("Expected embedded artwork"),
            }
        }
    }

    /// Test external artwork file detection.
    #[test]
    #[ignore = "Requires test directory structure"]
    fn test_external_artwork_detection() {
        let test_dir = Path::new("testdata/album_with_folder_jpg");
        if test_dir.exists() {
            // Create a dummy audio file in the test directory
            let audio_file = test_dir.join("track.flac");
            if audio_file.exists() {
                let result = extract_artwork(&audio_file);
                assert!(result.is_ok());
                match result.unwrap() {
                    External(path) => {
                        assert!(path.exists());
                        assert_eq!(path.file_name().unwrap(), "folder.jpg");
                    }
                    _ => panic!("Expected external artwork"),
                }
            }
        }
    }

    /// Test artwork extraction fallback behavior.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_artwork_extraction_fallback() {
        let test_file = Path::new("testdata/audio_without_artwork.flac");
        if test_file.exists() {
            let result = extract_artwork(test_file);

            // Should fail gracefully if no artwork is found
            assert!(result.is_err());
        }
    }

    /// Test MIME type detection.
    #[test]
    fn test_mime_type_detection() {
        // Test JPEG detection
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        let mime_type = detect_mime_type(&jpeg_data);
        assert_eq!(mime_type, Some(Jpeg));

        // Test PNG detection
        let png_data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mime_type = detect_mime_type(&png_data);
        assert_eq!(mime_type, Some(Png));

        // Test unknown format
        let unknown_data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let mime_type = detect_mime_type(&unknown_data);
        assert_eq!(mime_type, None);
    }

    /// Test metadata extraction with artwork.
    #[test]
    #[ignore = "Requires test audio files"]
    fn test_metadata_extraction_with_artwork() {
        let test_file = Path::new("testdata/flac_with_artwork.flac");
        if test_file.exists() {
            let result = TagReader::read_metadata(test_file);
            assert!(result.is_ok());
            let metadata = result.unwrap();
            assert!(metadata.artwork.is_some());
            assert!(!metadata.artwork.unwrap().is_empty());
        }
    }
}

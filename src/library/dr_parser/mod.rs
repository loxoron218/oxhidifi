//! DR (Dynamic Range) value extraction from text files.
//!
//! This module implements robust DR value extraction from various text file formats
//! according to official DR meter specifications, with caching for performance.

use std::{path::Path, sync::Arc};

use tracing::{debug, warn};

use crate::{error::dr_error::DrError, library::database::LibraryDatabase};

mod cache;
mod extractor;

pub use {cache::AlbumDrCache, extractor::DrExtractor};

/// Main DR parsing coordinator.
///
/// The `DrParser` coordinates DR extraction, caching, and database updates
/// for album directories.
pub struct DrParser {
    /// DR extractor instance.
    extractor: DrExtractor,
    /// DR value cache.
    cache: AlbumDrCache,
    /// Database interface.
    database: Arc<LibraryDatabase>,
}

impl DrParser {
    /// Creates a new DR parser.
    ///
    /// # Arguments
    ///
    /// * `database` - Database interface.
    ///
    /// # Returns
    ///
    /// A new `DrParser` instance.
    pub fn new(database: Arc<LibraryDatabase>) -> Self {
        Self {
            extractor: DrExtractor::new(),
            cache: AlbumDrCache::new(),
            database,
        }
    }

    /// Parses DR value for an album directory.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Album directory path.
    ///
    /// # Returns
    ///
    /// A `Result` containing the DR value (if found) or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError` if parsing fails, but handles missing files gracefully.
    pub async fn parse_dr_for_album<P: AsRef<Path>>(
        &self,
        album_path: P,
    ) -> Result<Option<String>, DrError> {
        let album_path = album_path.as_ref();

        // Check cache first
        if let Some(cached_dr) = self.cache.get(album_path) {
            return Ok(Some(cached_dr));
        }

        // Look for DR files in the album directory
        let dr_files = self.extractor.find_dr_files(album_path)?;

        if dr_files.is_empty() {
            // No DR files found - this is not an error, just no DR value
            return Ok(None);
        }

        // Try each DR file until we find a valid one
        for dr_file in dr_files {
            match self.extractor.extract_dr_from_file(&dr_file) {
                Ok(dr_value) => {
                    // Cache the result
                    self.cache.insert(album_path, dr_value.clone());

                    // Update database
                    if let Err(e) = self
                        .database
                        .update_dr_value(album_path, Some(&dr_value))
                        .await
                    {
                        warn!(
                            "Failed to update DR value in database for {:?}: {}",
                            album_path, e
                        );
                    }

                    return Ok(Some(dr_value));
                }
                Err(e) => {
                    debug!("Failed to parse DR file {:?}: {}", dr_file, e);
                    // Continue to next file
                }
            }
        }

        // No valid DR value found in any file
        Ok(None)
    }

    /// Gets the current DR cache.
    ///
    /// # Returns
    ///
    /// A reference to the `AlbumDrCache`.
    pub fn cache(&self) -> &AlbumDrCache {
        &self.cache
    }

    /// Gets the DR extractor.
    ///
    /// # Returns
    ///
    /// A reference to the `DrExtractor`.
    pub fn extractor(&self) -> &DrExtractor {
        &self.extractor
    }
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use {tempfile::TempDir, tokio::main};

    use crate::library::{
        database::LibraryDatabase,
        dr_parser::{DrExtractor, DrParser},
    };

    #[test]
    fn test_dr_extractor_patterns() {
        let extractor = DrExtractor::new();

        let test_cases = vec![
            ("DR12", true),
            ("DR 12", true),
            ("DR=12", true),
            ("Dynamic Range: 12", true),
            ("dr12", true),
            ("DR05", true),
            ("DR25", false), // Out of reasonable range
            ("DR", false),
            ("12", false),
            ("", false),
        ];

        for (input, expected_valid) in test_cases {
            let is_valid = extractor.validate_dr_value(input);
            assert_eq!(is_valid, expected_valid, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_dr_extraction_from_content() {
        let extractor = DrExtractor::new();

        let test_contents = vec![
            "DR12",
            "DR 12",
            "DR=12",
            "Dynamic Range: 12",
            "Some text\nDR12\nMore text",
            "Track 1: DR12\nTrack 2: DR10",
        ];

        for content in test_contents {
            let result = extractor.extract_dr_from_content(content);
            assert!(result.is_ok(), "Failed to extract from: {}", content);
            assert_eq!(result.unwrap(), "DR12");
        }
    }

    #[test]
    async fn test_dr_parser_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let album_dir = temp_dir.path();

        // Create a DR file
        let dr_file = album_dir.join("dr.txt");
        write(&dr_file, "DR12").unwrap();

        // Test file finding
        let files = DrParser::new(LibraryDatabase::new().await.unwrap().into())
            .extractor
            .find_dr_files(album_dir)
            .unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f == &dr_file));
    }
}

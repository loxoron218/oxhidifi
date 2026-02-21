//! DR (Dynamic Range) value extraction from text files.
//!
//! This module implements robust DR value extraction from various text file formats
//! according to official DR meter specifications, with caching for performance.

use std::{collections::HashSet, path::Path, sync::Arc};

use tracing::{debug, warn};

use crate::{error::dr_error::DrError, library::database::LibraryDatabase};

mod cache;
mod extractor;

pub use {cache::AlbumDrCache, extractor::DrExtractor};

/// Main DR parsing coordinator.
///
/// The `DrParser` coordinates DR extraction, caching, and database updates
/// for album directories.
#[derive(Debug, Clone)]
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
    /// A Result containing the new `DrParser` instance or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError::RegexError` if DR extractor initialization fails.
    pub fn new(database: Arc<LibraryDatabase>) -> Result<Self, DrError> {
        Ok(Self {
            extractor: DrExtractor::new()?,
            cache: AlbumDrCache::new(),
            database,
        })
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
    ///
    /// # Panics
    ///
    /// Panics if no DR values are found but valid DR files exist (should not happen with valid data).
    pub async fn parse_dr_for_album<P: AsRef<Path>>(
        &self,
        album_path: P,
    ) -> Result<Option<String>, DrError> {
        let album_path = album_path.as_ref();

        // Look for DR files in the album directory
        let dr_files = self.extractor.find_dr_files(album_path)?;

        if dr_files.is_empty() {
            // No DR files found - clear cache and return None
            self.cache.remove(album_path);
            return Ok(None);
        }

        // Check cache first (only after confirming files exist)
        if let Some(cached_dr) = self.cache.get(album_path) {
            return Ok(Some(cached_dr));
        }

        // Collect all valid DR values from all files
        let mut valid_dr_values = Vec::new();
        let mut parsing_errors = Vec::new();

        for dr_file in dr_files {
            match self.extractor.extract_dr_from_file(&dr_file) {
                Ok(dr_value) => {
                    valid_dr_values.push(dr_value);
                }
                Err(e) => {
                    debug!("Failed to parse DR file {:?}: {}", dr_file, e);
                    parsing_errors.push(e);
                }
            }
        }

        if valid_dr_values.is_empty() {
            if parsing_errors.is_empty() {
                // No DR files found or no valid DR values
                return Ok(None);
            }

            // All files failed to parse - this might indicate corrupted files
            // But we treat this as no DR value found (not an error)
            return Ok(None);
        }

        // Remove duplicates and find the highest DR value
        let unique_values: HashSet<String> = valid_dr_values.into_iter().collect();
        let mut sorted_values: Vec<String> = unique_values.into_iter().collect();

        // Sort by numeric DR value (higher is better)
        sorted_values.sort_by(|a, b| {
            let a_num = a
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);
            let b_num = b
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);
            b_num.cmp(&a_num) // Higher DR values first
        });

        let best_dr_value = sorted_values
            .first()
            .cloned()
            .ok_or(DrError::NoDrValueFound)?;

        // Cache the result
        self.cache.insert(album_path, best_dr_value.clone());

        // Update database
        if let Err(e) = self
            .database
            .update_dr_value(album_path, Some(&best_dr_value))
            .await
        {
            warn!(
                "Failed to update DR value in database for {:?}: {}",
                album_path, e
            );
        }

        Ok(Some(best_dr_value))
    }

    /// Gets the current DR cache.
    ///
    /// # Returns
    ///
    /// A reference to the `AlbumDrCache`.
    #[must_use]
    pub fn cache(&self) -> &AlbumDrCache {
        &self.cache
    }

    /// Gets the DR extractor.
    ///
    /// # Returns
    ///
    /// A reference to the `DrExtractor`.
    #[must_use]
    pub fn extractor(&self) -> &DrExtractor {
        &self.extractor
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{remove_file, write};

    use {
        anyhow::{Result, bail},
        tempfile::TempDir,
    };

    use crate::library::{
        database::LibraryDatabase,
        dr_parser::{DrExtractor, DrParser},
    };

    #[test]
    fn test_dr_extractor_patterns() -> Result<()> {
        let extractor = DrExtractor::new()?;

        let test_cases = vec![
            ("DR12", true),
            ("DR05", true),
            ("DR25", false), // Out of reasonable range
            ("DR", false),
            ("12", false),
            ("", false),
            ("DR 12", false),             // Not canonical format
            ("DR=12", false),             // Not canonical format
            ("Dynamic Range: 12", false), // Not canonical format
        ];

        for (input, expected_valid) in test_cases {
            let is_valid = extractor.validate_dr_value(input);
            if is_valid != expected_valid {
                bail!("Failed for input: {input}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_dr_extraction_from_content() -> Result<()> {
        let extractor = DrExtractor::new()?;

        // Test cases that should return DR12 (Official DR Value format only)
        let dr12_test_cases = vec![
            "Official DR value: DR12",
            "Official DR Value: DR12",
            "Some text\nOfficial DR value: DR12\nMore text",
        ];

        for content in dr12_test_cases {
            let result = extractor.extract_dr_from_content(content);
            if result.is_err() {
                bail!("Failed to extract from: {content}");
            }
            let extracted = result?;
            if extracted != "DR12" {
                bail!("Content: {content}");
            }
        }

        // Test official format cases with their expected values
        let official_format_cases = vec![
            ("Official DR value: DR6", "DR6"),
            ("Official EP/Album DR: 5", "DR5"),
            ("Official DR Value: DR9", "DR9"),
            ("Реальные значения DR:	DR5", "DR5"),
            // Additional edge cases for official formats
            ("Official DR value:DR8", "DR8"),
            ("Official EP/Album DR:12", "DR12"),
        ];

        for (content, expected) in official_format_cases {
            let result = extractor.extract_dr_from_content(content);
            if result.is_err() {
                bail!("Failed to extract from: {content}");
            }
            let extracted = result?;
            if extracted != expected {
                bail!("Content: {content}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_per_track_dr_values_rejected() -> Result<()> {
        let extractor = DrExtractor::new()?;

        // These should all fail to extract since they're per-track values, not Official DR Values
        let per_track_cases = vec![
            "Track 1: DR12",
            "DR12\nTrack 2: DR10",
            "Some content with DR8 embedded",
            "DR=12",
            "Dynamic Range: 12",
        ];

        for content in per_track_cases {
            let result = extractor.extract_dr_from_content(content);
            if result.is_ok() {
                bail!("Should not extract from per-track content: {content}");
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_dr_parser_with_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path();

        // Create various DR files including irregular names
        let dr_file1 = album_dir.join("dr.txt");
        let dr_file2 = album_dir.join("2012–2017_log.txt");
        let dr_file3 = album_dir.join("analysis.log");

        write(&dr_file1, "Official DR value: DR12")?;
        write(&dr_file2, "Official DR value: DR9")?;
        write(&dr_file3, "Some other content")?;

        // Test file finding - should find all text files (no database needed)
        let extractor = DrExtractor::new()?;
        let files = extractor.find_dr_files(album_dir)?;

        if files.len() != 3 {
            bail!("Expected 3 files, got {}", files.len());
        }
        if !files.iter().any(|f| f == &dr_file1) {
            bail!("DR file 1 not found");
        }
        if !files.iter().any(|f| f == &dr_file2) {
            bail!("DR file 2 not found");
        }
        if !files.iter().any(|f| f == &dr_file3) {
            bail!("DR file 3 not found");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_dr_parser_extraction_from_irregular_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path();

        // Create irregular filename with official DR format
        let irregular_file = album_dir.join("2012–2017_log.txt");
        write(&irregular_file, "Official DR value: DR9")?;

        // Test full parsing
        let database = LibraryDatabase::new().await?;
        let parser = DrParser::new(database.into())?;
        let result = parser.parse_dr_for_album(album_dir).await?;

        if result != Some("DR9".to_string()) {
            bail!("Expected DR9, got {result:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_dr_parser_multiple_values_conflict_resolution() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path();

        // Create multiple files with different DR values
        let file1 = album_dir.join("analysis1.txt");
        let file2 = album_dir.join("analysis2.txt");
        write(&file1, "Official DR value: DR6")?;
        write(&file2, "Official DR value: DR9")?;

        // Test conflict resolution - should return highest DR value
        let database = LibraryDatabase::new().await?;
        let parser = DrParser::new(database.into())?;
        let result = parser.parse_dr_for_album(album_dir).await?;

        if result != Some("DR9".to_string()) {
            bail!("Expected DR9, got {result:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_dr_parser_duplicate_values() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path();

        // Create multiple files with same DR value
        let file1 = album_dir.join("analysis1.txt");
        let file2 = album_dir.join("analysis2.txt");
        write(&file1, "Official DR value: DR8")?;
        write(&file2, "Official DR Value: DR8")?;

        // Test duplicate handling
        let database = LibraryDatabase::new().await?;
        let parser = DrParser::new(database.into())?;
        let result = parser.parse_dr_for_album(album_dir).await?;

        if result != Some("DR8".to_string()) {
            bail!("Expected DR8, got {result:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_dr_parser_file_removal_clears_value() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path();

        // Create a DR file
        let dr_file = album_dir.join("dr.txt");
        write(&dr_file, "Official DR value: DR7")?;

        // Parse initially - should get DR7
        let database = LibraryDatabase::new().await?;
        let parser = DrParser::new(database.clone().into())?;
        let result = parser.parse_dr_for_album(album_dir).await?;
        if result != Some("DR7".to_string()) {
            bail!("Expected DR7, got {result:?}");
        }

        // Remove the DR file
        remove_file(&dr_file)?;

        // Parse again - should get None
        let result2 = parser.parse_dr_for_album(album_dir).await?;
        if result2.is_some() {
            bail!("Expected None, got {result2:?}");
        }

        // Verify database was updated to clear the value
        let db_dr_value = database.get_dr_value(album_dir).await?;
        if db_dr_value.is_some() {
            bail!("Expected database DR value None, got {db_dr_value:?}");
        }
        Ok(())
    }
}

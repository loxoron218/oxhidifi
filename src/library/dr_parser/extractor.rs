//! DR value extraction and validation logic.

use std::path::Path;

use {
    regex::Regex,
};

use crate::error::dr_error::DrError;

/// DR value validation regex pattern.
const DR_VALUE_PATTERN: &str = r"^DR(\d{1,2})$";

/// Supported DR file patterns.
const DR_FILE_PATTERNS: &[&str] = &[
    "dr*.txt",
    "*dr*.txt",
    "dr*",
    "*dr*",
];

/// Extracts and validates DR values from album directories.
///
/// The `DrExtractor` parses various DR meter log formats and extracts
/// valid DR values according to official specifications.
pub struct DrExtractor {
    /// Regex patterns for different DR log formats.
    dr_patterns: Vec<Regex>,
    /// DR value validation regex.
    dr_validator: Regex,
}

impl DrExtractor {
    /// Creates a new DR extractor.
    ///
    /// # Returns
    ///
    /// A new `DrExtractor` instance.
    pub fn new() -> Self {
        let mut dr_patterns = Vec::new();

        // Add patterns for common DR log formats
        dr_patterns.push(Regex::new(r"(?i)^\s*DR\s*(\d{1,2})\s*$").unwrap());
        dr_patterns.push(Regex::new(r"(?i)DR\s*=\s*(\d{1,2})").unwrap());
        dr_patterns.push(Regex::new(r"(?i)Dynamic Range\s*[:=]\s*(\d{1,2})").unwrap());
        dr_patterns.push(Regex::new(r"(?i)^\s*(DR\d{1,2})\s*$").unwrap());

        let dr_validator = Regex::new(DR_VALUE_PATTERN).unwrap();

        Self {
            dr_patterns,
            dr_validator,
        }
    }

    /// Extracts DR value from a DR file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the DR file.
    ///
    /// # Returns
    ///
    /// A `Result` containing the extracted DR value or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError` if the file cannot be read or parsed.
    pub fn extract_dr_from_file<P: AsRef<Path>>(&self, file_path: P) -> Result<String, DrError> {
        let content = std::fs::read_to_string(file_path)?;
        self.extract_dr_from_content(&content)
    }

    /// Extracts DR value from content string.
    ///
    /// # Arguments
    ///
    /// * `content` - Content to parse for DR value.
    ///
    /// # Returns
    ///
    /// A `Result` containing the extracted DR value or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError` if no valid DR value can be extracted.
    pub fn extract_dr_from_content(&self, content: &str) -> Result<String, DrError> {
        // Split content into lines and search for DR patterns
        for line in content.lines() {
            for pattern in &self.dr_patterns {
                if let Some(captures) = pattern.captures(line) {
                    let dr_number = &captures[1];
                    let dr_value = format!("DR{}", dr_number);
                    
                    // Validate the extracted DR value
                    if self.validate_dr_value(&dr_value) {
                        return Ok(dr_value);
                    }
                }
            }
        }

        Err(DrError::NoDrValueFound)
    }

    /// Validates a DR value against the expected format.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - DR value to validate.
    ///
    /// # Returns
    ///
    /// `true` if the DR value is valid, `false` otherwise.
    pub fn validate_dr_value(&self, dr_value: &str) -> bool {
        if !self.dr_validator.is_match(dr_value) {
            return false;
        }

        // Extract the number part and validate range (1-20 is reasonable)
        if let Some(captures) = self.dr_validator.captures(dr_value) {
            if let Ok(number) = captures[1].parse::<u32>() {
                return number >= 1 && number <= 20;
            }
        }

        false
    }

    /// Finds potential DR files in an album directory.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Album directory path.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of potential DR file paths or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError` if the directory cannot be read.
    pub fn find_dr_files<P: AsRef<Path>>(&self, album_path: P) -> Result<Vec<std::path::PathBuf>, DrError> {
        let album_path = album_path.as_ref();
        
        if !album_path.exists() || !album_path.is_dir() {
            return Ok(Vec::new());
        }

        let mut dr_files = Vec::new();
        
        for entry in std::fs::read_dir(album_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // Check against supported DR file patterns
                    for pattern in DR_FILE_PATTERNS {
                        if self.matches_dr_pattern(filename, pattern) {
                            dr_files.push(path.clone());
                            break;
                        }
                    }
                }
            }
        }

        Ok(dr_files)
    }

    /// Checks if a filename matches a DR file pattern.
    ///
    /// # Arguments
    ///
    /// * `filename` - Filename to check.
    /// * `pattern` - Pattern to match against.
    ///
    /// # Returns
    ///
    /// `true` if the filename matches the pattern, `false` otherwise.
    fn matches_dr_pattern(&self, filename: &str, pattern: &str) -> bool {
        // Simple glob-like pattern matching
        if pattern == "dr*.txt" {
            return filename.to_lowercase().starts_with("dr") && filename.ends_with(".txt");
        } else if pattern == "*dr*.txt" {
            return filename.contains("dr") && filename.ends_with(".txt");
        } else if pattern == "dr*" {
            return filename.to_lowercase().starts_with("dr");
        } else if pattern == "*dr*" {
            return filename.contains("dr");
        }
        
        false
    }
}
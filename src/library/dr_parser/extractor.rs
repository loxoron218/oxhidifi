//! DR value extraction and validation logic.

use std::{
    fs::{read_dir, read_to_string},
    path::{Path, PathBuf},
};

use regex::Regex;

use crate::error::dr_error::DrError::{self, NoDrValueFound, RegexError};

/// DR value validation regex pattern.
const DR_VALUE_PATTERN: &str = r"^DR(\d{1,2})$";

/// Supported text file extensions for DR value scanning.
const TEXT_FILE_EXTENSIONS: &[&str] = &["txt", "log", "md", "csv"];

/// Extracts and validates DR values from album directories.
///
/// The `DrExtractor` parses various DR meter log formats and extracts
/// valid DR values according to official specifications.
#[derive(Debug, Clone)]
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
    /// A `Result` containing the new `DrExtractor` instance or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError::RegexError` if any of the hardcoded regex patterns
    /// fail to compile. This indicates a bug in the pattern definitions.
    pub fn new() -> Result<Self, DrError> {
        // Add patterns for Official DR value formats only
        // All patterns should capture only the numeric part (group 1)
        // Per the specification, we only extract Official DR Values, not per-track DR values
        let dr_patterns = vec![
            // Official DR value patterns from spec (docs/0. dr-extraction.txt)
            compile_regex(r"(?i)Official\s+DR\s+value[:\s]*DR(\d{1,2})")?,
            compile_regex(r"(?i)Official\s+EP/Album\s+DR[:\s]*(\d{1,2})")?,
            compile_regex(r"(?i)Official\s+DR\s+Value[:\s]*DR(\d{1,2})")?,
            compile_regex(r"(?i)Реальные\s+значения\s+DR[:\s]*DR(\d{1,2})")?,
        ];

        let dr_validator = compile_regex(DR_VALUE_PATTERN)?;

        Ok(Self {
            dr_patterns,
            dr_validator,
        })
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
        let content = read_to_string(file_path)?;
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
                    let dr_value = format!("DR{dr_number}");

                    // Validate the extracted DR value
                    if self.validate_dr_value(&dr_value) {
                        return Ok(dr_value);
                    }
                }
            }
        }

        Err(NoDrValueFound)
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
    #[must_use]
    pub fn validate_dr_value(&self, dr_value: &str) -> bool {
        // Only validate the canonical format (DR12)
        // Per specification, we only accept Official DR Values in canonical format
        if self.dr_validator.is_match(dr_value)
            && let Some(captures) = self.dr_validator.captures(dr_value)
            && let Ok(number) = captures[1].parse::<u32>()
        {
            return (1..=20).contains(&number);
        }

        false
    }

    /// Finds potential DR files in an album directory.
    ///
    /// Scans all text files in the directory for potential DR values,
    /// since DR log files can have irregular names (e.g., "`2012–2017_log.txt`").
    ///
    /// # Arguments
    ///
    /// * `album_path` - Album directory path.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of potential text file paths or a `DrError`.
    ///
    /// # Errors
    ///
    /// Returns `DrError` if the directory cannot be read.
    pub fn find_dr_files<P: AsRef<Path>>(album_path: P) -> Result<Vec<PathBuf>, DrError> {
        let album_path = album_path.as_ref();

        if !album_path.exists() || !album_path.is_dir() {
            return Ok(Vec::new());
        }

        let mut text_files = Vec::new();

        for entry in read_dir(album_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file()
                && let Some(extension) = path.extension().and_then(|ext| ext.to_str())
            {
                // Check against supported text file extensions
                if TEXT_FILE_EXTENSIONS
                    .iter()
                    .any(|&ext| ext.eq_ignore_ascii_case(extension))
                {
                    text_files.push(path.clone());
                }
            }
        }

        Ok(text_files)
    }
}

/// Compiles a regex pattern, returning a `DrError::RegexError` on failure.
///
/// # Arguments
///
/// * `pattern` - The regex pattern string to compile.
///
/// # Returns
///
/// A `Result` containing the compiled `Regex` or a `DrError::RegexError`.
///
/// # Errors
///
/// Returns `DrError::RegexError` if the pattern is invalid.
fn compile_regex(pattern: &str) -> Result<Regex, DrError> {
    Regex::new(pattern).map_err(|e| RegexError {
        pattern: pattern.to_string(),
        error: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, bail};

    use crate::{
        error::dr_error::DrError::RegexError, library::dr_parser::extractor::compile_regex,
    };

    #[test]
    fn test_invalid_regex_pattern_error_message() -> Result<()> {
        let result = compile_regex("(invalid[regex");

        let error = result
            .err()
            .ok_or_else(|| anyhow!("Expected error but got Ok"))?;

        if let RegexError {
            pattern,
            error: err_msg,
        } = error
        {
            if pattern != "(invalid[regex" {
                bail!("Expected pattern '(invalid[regex', got '{pattern}'");
            }
            if !err_msg.contains("regex") {
                bail!("Error message should contain 'regex': {err_msg}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_valid_regex_pattern_compiles() -> Result<()> {
        let result = compile_regex(r"^DR(\d{1,2})$")?;

        drop(result);
        Ok(())
    }
}

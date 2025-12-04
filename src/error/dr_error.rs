//! Domain-specific DR parsing errors.
//!
//! This module defines error types specific to DR (Dynamic Range) value parsing
//! operations, extending the existing error system.

use {
    std::fmt::Display,
    thiserror::Error,
};

/// Error type for DR parsing operations.
#[derive(Error, Debug)]
pub enum DrError {
    /// Failed to read or parse a DR file.
    #[error("Failed to read DR file: {0}")]
    ReadError(#[from] std::io::Error),
    /// The DR file content is malformed or invalid.
    #[error("Invalid DR file content: {reason}")]
    InvalidContent { reason: String },
    /// No valid DR value could be extracted from the file.
    #[error("No valid DR value found in file")]
    NoDrValueFound,
    /// The extracted DR value does not match expected format.
    #[error("Invalid DR value format: {value}")]
    InvalidDrFormat { value: String },
}

impl DrError {
    /// Creates a new `InvalidContent` error.
    ///
    /// # Arguments
    ///
    /// * `reason` - Reason for the invalid content.
    ///
    /// # Returns
    ///
    /// A new `DrError::InvalidContent`.
    pub fn invalid_content(reason: impl Into<String>) -> Self {
        Self::InvalidContent {
            reason: reason.into(),
        }
    }

    /// Creates a new `InvalidDrFormat` error.
    ///
    /// # Arguments
    ///
    /// * `value` - The invalid DR value.
    ///
    /// # Returns
    ///
    /// A new `DrError::InvalidDrFormat`.
    pub fn invalid_dr_format(value: impl Into<String>) -> Self {
        Self::InvalidDrFormat {
            value: value.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::dr_error::DrError;

    #[test]
    fn test_dr_error_display() {
        let read_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let dr_error = DrError::ReadError(read_error);
        assert!(dr_error.to_string().contains("Failed to read DR file"));

        let invalid_content_error = DrError::invalid_content("Missing required fields");
        assert_eq!(
            invalid_content_error.to_string(),
            "Invalid DR file content: Missing required fields"
        );

        let no_dr_value_error = DrError::NoDrValueFound;
        assert_eq!(no_dr_value_error.to_string(), "No valid DR value found in file");

        let invalid_format_error = DrError::invalid_dr_format("DR123");
        assert_eq!(
            invalid_format_error.to_string(),
            "Invalid DR value format: DR123"
        );
    }
}
//! Domain-specific error types using `thiserror`.
//!
//! This module defines the main error enums for different domains
//! within the Oxhidifi application: audio, library, and UI operations.

use std::result::Result as StdResult;

use {anyhow::Error, sqlx::Error as SqlxError, thiserror::Error};

use crate::{
    audio::{decoder::DecoderError, metadata::MetadataError, output::OutputError},
    error::dr_error::DrError,
    library::{database::LibraryError as DatabaseError, schema::SchemaError},
};

/// Audio-related errors.
#[derive(Error, Debug)]
pub enum AudioError {
    /// Decoder error from the audio decoder module.
    #[error("Decoder error: {0}")]
    DecoderError(#[from] DecoderError),
    /// Output error from the audio output module.
    #[error("Output error: {0}")]
    OutputError(#[from] OutputError),
    /// Metadata error from the metadata extraction module.
    #[error("Metadata error: {0}")]
    MetadataError(#[from] MetadataError),
    /// Invalid operation for current state.
    #[error("Invalid operation: {reason}")]
    InvalidOperation { reason: String },
    /// Track not found or not loaded.
    #[error("No track loaded")]
    NoTrackLoaded,
}

/// Library-related errors.
#[derive(Error, Debug)]
pub enum LibraryError {
    /// Database connection or query error.
    #[error("Database error: {0}")]
    DatabaseError(#[from] SqlxError),
    /// Schema initialization error.
    #[error("Schema error: {0}")]
    SchemaError(#[from] SchemaError),
    /// Invalid file path or metadata.
    #[error("Invalid data: {reason}")]
    InvalidData { reason: String },
    /// Record not found.
    #[error("Record not found: {entity} with id {id}")]
    NotFound { entity: String, id: i64 },
    /// DR parsing error.
    #[error("DR parsing error: {0}")]
    DrError(#[from] DrError),
    /// Database library error.
    #[error("Database library error: {0}")]
    DatabaseLibraryError(#[from] DatabaseError),
}

/// UI-related errors.
#[derive(Error, Debug)]
pub enum UiError {
    /// GTK/Libadwaita initialization error.
    #[error("UI initialization error: {0}")]
    InitializationError(String),
    /// Widget creation error.
    #[error("Widget creation error: {0}")]
    WidgetError(String),
    /// State update error.
    #[error("State update error: {0}")]
    StateError(String),
}

/// Operational error context propagation with `anyhow`.
///
/// This type is used for operational errors that need rich context
/// but don't require specific handling logic.
pub type Result<T> = StdResult<T, Error>;

#[cfg(test)]
mod tests {
    use crate::error::domain::{AudioError, LibraryError, UiError};

    #[test]
    fn test_audio_error_display() {
        let no_track_error = AudioError::NoTrackLoaded;
        assert_eq!(no_track_error.to_string(), "No track loaded");

        let invalid_op_error = AudioError::InvalidOperation {
            reason: "test reason".to_string(),
        };
        assert_eq!(
            invalid_op_error.to_string(),
            "Invalid operation: test reason"
        );
    }

    #[test]
    fn test_library_error_display() {
        let not_found_error = LibraryError::NotFound {
            entity: "album".to_string(),
            id: 123,
        };
        assert_eq!(
            not_found_error.to_string(),
            "Record not found: album with id 123"
        );

        let invalid_data_error = LibraryError::InvalidData {
            reason: "test reason".to_string(),
        };
        assert_eq!(invalid_data_error.to_string(), "Invalid data: test reason");
    }

    #[test]
    fn test_ui_error_display() {
        let init_error = UiError::InitializationError("Failed to init GTK".to_string());
        assert_eq!(
            init_error.to_string(),
            "UI initialization error: Failed to init GTK"
        );

        let widget_error = UiError::WidgetError("Failed to create button".to_string());
        assert_eq!(
            widget_error.to_string(),
            "Widget creation error: Failed to create button"
        );
    }
}

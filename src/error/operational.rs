//! Operational error context propagation with `anyhow`.
//!
//! This module provides extension traits and utilities for enhancing
//! error context and centralized error reporting.

use anyhow::{Context, Result as AnyhowResult};

/// Extension trait for enhanced error context.
///
/// This trait provides methods to add contextual information to errors,
/// making debugging and user feedback more informative.
pub trait ResultExt<T, E> {
    /// Adds context to an error with a static string.
    fn add_context(self, context: &'static str) -> AnyhowResult<T>
    where
        E: std::error::Error + Send + Sync + 'static;

    /// Adds context to an error with a formatted string.
    fn add_contextf(self, format: impl std::fmt::Display) -> AnyhowResult<T>
    where
        E: std::error::Error + Send + Sync + 'static;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn add_context(self, context: &'static str) -> AnyhowResult<T>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.context(context)
    }

    fn add_contextf(self, format: impl std::fmt::Display) -> AnyhowResult<T>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.context(format.to_string())
    }
}

/// Centralized error reporting and logging.
///
/// The `ErrorReporter` provides a consistent interface for logging
/// errors at different severity levels and reporting them to users.
pub struct ErrorReporter;

impl ErrorReporter {
    /// Reports a debug-level error (development only).
    pub fn debug(error: &anyhow::Error, context: &str) {
        tracing::debug!(context = context, error = %error, "Debug error");
    }

    /// Reports an info-level error (user actions and system events).
    pub fn info(error: &anyhow::Error, context: &str) {
        tracing::info!(context = context, error = %error, "Info error");
    }

    /// Reports a warning-level error (recoverable issues).
    pub fn warn(error: &anyhow::Error, context: &str) {
        tracing::warn!(context = context, error = %error, "Warning error");
    }

    /// Reports an error-level error (non-recoverable issues).
    pub fn error(error: &anyhow::Error, context: &str) {
        tracing::error!(context = context, error = %error, "Error error");
    }

    /// Converts an error to a user-friendly message.
    ///
    /// This method extracts the most relevant information from an error
    /// chain and formats it for display to end users.
    pub fn to_user_message(error: &anyhow::Error) -> String {
        // For now, just return the top-level error message
        // In a more sophisticated implementation, we'd have specific
        // user-friendly messages for different error types
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn test_result_ext_with_context() {
        #[derive(Debug)]
        struct TestError;
        impl std::fmt::Display for TestError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Test error")
            }
        }
        impl std::error::Error for TestError {}
        
        let result: Result<i32, TestError> = Err(TestError);
        let with_context = result.add_context("Additional context");
        
        assert!(with_context.is_err());
        let error = with_context.unwrap_err();
        // The error should contain the context, not necessarily the original error message
        assert!(error.to_string().contains("Additional context"));
    }

    #[test]
    fn test_result_ext_with_contextf() {
        #[derive(Debug)]
        struct TestError;
        impl std::fmt::Display for TestError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Test error")
            }
        }
        impl std::error::Error for TestError {}
        
        let result: Result<i32, TestError> = Err(TestError);
        let with_context = result.add_contextf("Formatted context: test");
        
        assert!(with_context.is_err());
        let error = with_context.unwrap_err();
        // The error should contain the context, not necessarily the original error message
        assert!(error.to_string().contains("Formatted context: test"));
    }

    #[test]
    fn test_error_reporter_user_message() {
        let error = anyhow!("Test error message");
        let user_message = ErrorReporter::to_user_message(&error);
        assert_eq!(user_message, "Test error message");
    }
}
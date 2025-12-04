//! Operational error context propagation with `anyhow`.
//!
//! This module provides extension traits and utilities for enhancing
//! error context and centralized error reporting.

use std::{error::Error as StdError, fmt::Display};

use {
    anyhow::{Context, Error, Result as AnyhowResult},
    tracing::{debug, error, info, warn},
};

/// Extension trait for enhanced error context.
///
/// This trait provides methods to add contextual information to errors,
/// making debugging and user feedback more informative.
pub trait ResultExt<T, E> {
    /// Adds context to an error with a static string.
    fn add_context(self, context: &'static str) -> AnyhowResult<T>
    where
        E: StdError + Send + Sync + 'static;

    /// Adds context to an error with a formatted string.
    fn add_contextf(self, format: impl Display) -> AnyhowResult<T>
    where
        E: StdError + Send + Sync + 'static;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn add_context(self, context: &'static str) -> AnyhowResult<T>
    where
        E: StdError + Send + Sync + 'static,
    {
        self.context(context)
    }

    fn add_contextf(self, format: impl Display) -> AnyhowResult<T>
    where
        E: StdError + Send + Sync + 'static,
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
    pub fn debug(error: &Error, context: &str) {
        debug!(context = context, error = %error, "Debug error");
    }

    /// Reports an info-level error (user actions and system events).
    pub fn info(error: &Error, context: &str) {
        info!(context = context, error = %error, "Info error");
    }

    /// Reports a warning-level error (recoverable issues).
    pub fn warn(error: &Error, context: &str) {
        warn!(context = context, error = %error, "Warning error");
    }

    /// Reports an error-level error (non-recoverable issues).
    pub fn error(error: &Error, context: &str) {
        error!(context = context, error = %error, "Error error");
    }

    /// Converts an error to a user-friendly message.
    ///
    /// This method extracts the most relevant information from an error
    /// chain and formats it for display to end users.
    pub fn to_user_message(error: &Error) -> String {
        // For now, just return the top-level error message
        // In a more sophisticated implementation, we'd have specific
        // user-friendly messages for different error types
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fmt::{Display, Formatter, Result as FmtResult},
    };

    use anyhow::anyhow;

    use crate::error::operational::{ErrorReporter, ResultExt};

    #[test]
    fn test_result_ext_with_context() {
        #[derive(Debug)]
        struct TestError;
        impl Display for TestError {
            fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
                write!(f, "Test error")
            }
        }
        impl Error for TestError {}

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
        impl Display for TestError {
            fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
                write!(f, "Test error")
            }
        }
        impl Error for TestError {}

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

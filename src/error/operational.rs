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
    ///
    /// # Returns
    ///
    /// A `Result` with additional context.
    ///
    /// # Errors
    ///
    /// Returns the original error wrapped with additional context.
    fn add_context(self, context: &'static str) -> AnyhowResult<T>
    where
        E: StdError + Send + Sync + 'static;

    /// Adds context to an error with a formatted string.
    ///
    /// # Returns
    ///
    /// A `Result` with additional context.
    ///
    /// # Errors
    ///
    /// Returns the original error wrapped with additional context.
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
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fmt::{Display, Formatter, Result as FmtResult},
    };

    use anyhow::{Result, anyhow, bail};

    use crate::error::operational::ResultExt;

    #[test]
    fn test_result_ext_with_context() -> Result<()> {
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

        if with_context.is_ok() {
            bail!("Add_context should return Err for error input");
        }
        let error = with_context
            .err()
            .ok_or_else(|| anyhow!("Failed to extract error from Err variant"))?;

        if !error.to_string().contains("Additional context") {
            bail!("Error should contain context, got: {error}");
        }
        Ok(())
    }

    #[test]
    fn test_result_ext_with_contextf() -> Result<()> {
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

        if with_context.is_ok() {
            bail!("Add_contextf should return Err for error input");
        }
        let error = with_context
            .err()
            .ok_or_else(|| anyhow!("Failed to extract error from Err variant"))?;

        if !error.to_string().contains("Formatted context: test") {
            bail!("Error should contain formatted context, got: {error}");
        }
        Ok(())
    }
}

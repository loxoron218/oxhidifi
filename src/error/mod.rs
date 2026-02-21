//! Comprehensive error handling system using `thiserror` and `anyhow`.
//!
//! This module provides both domain-specific error types for precise error
//! handling and operational error context propagation for rich debugging information.

pub mod audio_reporting;
pub mod domain;
pub mod dr_error;
pub mod numeric_conversion;
pub mod operational;

pub use {
    audio_reporting::handle_exclusive_mode_error,
    domain::{AudioError, LibraryError, UiError},
    operational::{ErrorReporter, ResultExt},
};

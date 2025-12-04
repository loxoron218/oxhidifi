//! Comprehensive error handling system using `thiserror` and `anyhow`.
//!
//! This module provides both domain-specific error types for precise error
//! handling and operational error context propagation for rich debugging information.

pub mod domain;
pub mod operational;

pub use {
    domain::{AudioError, LibraryError, UiError},
    operational::{ErrorReporter, ResultExt},
};

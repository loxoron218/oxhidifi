//! Centralized state management with reactive updates to UI components.
//!
//! This module provides the foundation for managing global application state
//! with thread-safe access and reactive update mechanisms.

pub mod app_state;

pub use app_state::{AppState, AppStateEvent, LibraryState, StateObserver, ViewMode};

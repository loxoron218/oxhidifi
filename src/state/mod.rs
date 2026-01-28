//! Centralized state management with reactive updates to UI components.
//!
//! This module provides the foundation for managing global application state
//! with thread-safe access and reactive update mechanisms.

pub mod app_state;
pub mod zoom_manager;

pub use {
    app_state::{
        AppState, AppStateEvent, LibraryState, LibraryTab, NavigationState, PlaybackQueue, ViewMode,
    },
    zoom_manager::{ZoomEvent, ZoomManager},
};

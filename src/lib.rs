//! Oxhidifi - High-Fidelity Music Player
//!
//! A high-fidelity, offline-first music player designed exclusively for audiophiles
//! and music enthusiasts. Built with modern Rust and Libadwaita, it provides
//! bit-perfect audio playback, comprehensive metadata display, and a responsive
//! GNOME-compliant user interface.

pub mod audio;
pub mod config;
pub mod error;
pub mod library;
pub mod state;
pub mod ui;

// Re-export key types for convenience
pub use {
    audio::engine::{AudioEngine, AudioError, PlaybackState, TrackInfo},
    config::{SettingsManager, UserSettings},
    error::{AudioError as Error, LibraryError, UiError},
    library::{Album, Artist, LibraryDatabase, SearchResults, Track},
    state::{AppState, AppStateEvent, LibraryState, ViewMode},
    ui::OxhidifiApplication,
};

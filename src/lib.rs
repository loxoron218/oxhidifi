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
pub use audio::engine::{AudioEngine, AudioError, PlaybackState, TrackInfo};
pub use config::{SettingsManager, UserSettings};
pub use error::{AudioError as Error, LibraryError, UiError};
pub use library::{Album, Artist, LibraryDatabase, SearchResults, Track};
pub use state::{AppState, AppStateEvent, LibraryState, StateObserver, ViewMode};
pub use ui::OxhidifiApplication;
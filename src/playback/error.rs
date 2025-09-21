use std::{
    error::Error,
    fmt::{Display, Formatter, Result},
    path::PathBuf,
};

use gstreamer::{LoggableError, StateChangeError, glib::BoolError};

use crate::playback::error::PlaybackError::{
    FileNotFound, GStreamer, GStreamerBool, GStreamerGLib, GStreamerState, InvalidState, Pipeline,
};

/// Errors that can occur during playback operations
///
/// This enum encompasses all possible error types that might occur during
/// audio playback, including GStreamer-specific errors, file system errors,
/// and application state errors.
#[derive(Debug)]
pub enum PlaybackError {
    /// GStreamer error
    GStreamer(LoggableError),
    /// GStreamer GLib error
    GStreamerGLib(gstreamer::glib::Error),
    /// GStreamer boolean error
    GStreamerBool(BoolError),
    /// GStreamer state change error
    GStreamerState(StateChangeError),
    /// Pipeline error
    Pipeline(String),
    /// File not found error
    FileNotFound(PathBuf),
    /// Invalid state error
    InvalidState(String),
    /// Database error
    DatabaseError(String),
}

impl Display for PlaybackError {
    /// Formats the error for display purposes
    ///
    /// This implementation provides user-friendly error messages for each
    /// variant of the `PlaybackError` enum.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            GStreamer(e) => write!(f, "GStreamer error: {}", e),
            GStreamerGLib(e) => write!(f, "GStreamer GLib error: {}", e),
            GStreamerBool(e) => write!(f, "GStreamer boolean error: {}", e),
            GStreamerState(e) => write!(f, "GStreamer state change error: {}", e),
            Pipeline(s) => write!(f, "Pipeline error: {}", s),
            FileNotFound(p) => write!(f, "File not found: {}", p.display()),
            InvalidState(s) => write!(f, "Invalid state: {}", s),
            PlaybackError::DatabaseError(s) => write!(f, "Database error: {}", s),
        }
    }
}

impl Error for PlaybackError {
    /// Returns the source of this error if available
    ///
    /// For GStreamer-related errors, this returns the underlying error.
    /// For other error types, this returns `None` as they don't have
    /// underlying sources.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GStreamer(e) => Some(e),
            GStreamerGLib(e) => Some(e),
            GStreamerBool(e) => Some(e),
            GStreamerState(e) => Some(e),
            Pipeline(_) => None,
            FileNotFound(_) => None,
            InvalidState(_) => None,
            PlaybackError::DatabaseError(_) => None,
        }
    }
}

// Manual From implementations to convert GStreamer errors to PlaybackError

impl From<LoggableError> for PlaybackError {
    /// Converts a `LoggableError` into a `PlaybackError::GStreamer`
    fn from(error: LoggableError) -> Self {
        GStreamer(error)
    }
}

impl From<gstreamer::glib::Error> for PlaybackError {
    /// Converts a GStreamer GLib error into a `PlaybackError::GStreamerGLib`
    fn from(error: gstreamer::glib::Error) -> Self {
        GStreamerGLib(error)
    }
}

impl From<BoolError> for PlaybackError {
    /// Converts a `BoolError` into a `PlaybackError::GStreamerBool`
    fn from(error: BoolError) -> Self {
        GStreamerBool(error)
    }
}

impl From<StateChangeError> for PlaybackError {
    /// Converts a `StateChangeError` into a `PlaybackError::GStreamerState`
    fn from(error: StateChangeError) -> Self {
        GStreamerState(error)
    }
}

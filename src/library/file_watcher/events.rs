//! File system event definitions and processing.

use std::path::PathBuf;

/// File system event that has been processed and filtered.
#[derive(Debug, Clone)]
pub enum ProcessedEvent {
    /// A file was created or modified.
    FileChanged {
        /// Path to the changed file.
        path: PathBuf,
        /// Whether this is a new file.
        is_new: bool,
    },
    /// A file was removed.
    FileRemoved {
        /// Path to the removed file.
        path: PathBuf,
    },
    /// A file was renamed/moved.
    FileRenamed {
        /// Original path of the file.
        from: PathBuf,
        /// New path of the file.
        to: PathBuf,
    },
}

/// Debounced file system event.
#[derive(Debug, Clone)]
pub enum DebouncedEvent {
    /// Files that have changed (created/modified).
    FilesChanged {
        /// Paths of changed files.
        paths: Vec<PathBuf>,
    },
    /// Files that have been removed.
    FilesRemoved {
        /// Paths of removed files.
        paths: Vec<PathBuf>,
    },
    /// Files that have been renamed/moved.
    FilesRenamed {
        /// Original and new paths of renamed files.
        paths: Vec<(PathBuf, PathBuf)>,
    },
}
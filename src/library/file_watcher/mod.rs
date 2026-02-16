//! File system change detection using the `notify` crate.
//!
//! This module provides real-time file system monitoring capabilities
//! for music library directories, with support for debouncing and
//! event filtering for supported audio formats.

use std::{
    collections::HashSet,
    fs::read_dir,
    path::{Path, PathBuf},
    sync::Arc,
};

use {
    async_channel::Sender,
    notify::{
        Config, Error, Event, RecommendedWatcher,
        RecursiveMode::Recursive,
        Watcher,
        event::{
            EventKind::{self, Create, Modify, Other, Remove},
            ModifyKind::{Data, Name},
            RenameMode::{self, Both, From, To},
        },
    },
    parking_lot::RwLock,
    tracing::{debug, error},
};

use crate::{audio::format_detector::supported_audio_extensions, error::domain::LibraryError};

mod config;
mod debouncer;
mod events;

pub use {
    config::FileWatcherConfig,
    debouncer::DebouncedEventProcessor,
    events::{
        DebouncedEvent,
        ProcessedEvent::{self, FileChanged, FileRemoved},
    },
};

/// Supported text file extensions for DR value monitoring.
const SUPPORTED_TEXT_EXTENSIONS: &[&str] = &["txt", "log", "md", "csv"];

/// File system watcher for music library directories.
///
/// The `FileWatcher` uses the `notify` crate to monitor file system changes
/// in specified music library directories. It filters events to only process
/// supported audio formats and applies debouncing to handle rapid changes.
#[derive(Debug)]
pub struct FileWatcher {
    /// Internal notify watcher.
    watcher: RecommendedWatcher,
    /// Set of currently watched paths.
    watched_paths: Arc<RwLock<HashSet<PathBuf>>>,
    /// Configuration for watcher behavior.
    config: FileWatcherConfig,
}

impl FileWatcher {
    /// Creates a new file watcher.
    ///
    /// # Arguments
    ///
    /// * `event_sender` - Channel sender for processed events.
    /// * `config` - Optional configuration (uses defaults if None).
    ///
    /// # Returns
    ///
    /// A `Result` containing the `FileWatcher` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the watcher cannot be initialized.
    pub fn new(
        event_sender: Sender<ProcessedEvent>,
        config: Option<FileWatcherConfig>,
    ) -> Result<Self, LibraryError> {
        let config = config.unwrap_or_default();

        // Create notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                Self::handle_raw_event(res, &event_sender.clone());
            },
            Config::default(),
        )
        .map_err(|e| LibraryError::InvalidData {
            reason: format!("Failed to create file watcher: {e}"),
        })?;

        // Apply configuration - hidden file handling is done in our event filter
        watcher
            .configure(Config::default())
            .map_err(|e| LibraryError::InvalidData {
                reason: format!("Failed to configure watcher: {e}"),
            })?;

        let file_watcher = Self {
            watcher,
            watched_paths: Arc::new(RwLock::new(HashSet::new())),
            config,
        };

        Ok(file_watcher)
    }

    /// Handles raw events from the notify crate.
    ///
    /// This method processes raw file system events, filters them based on
    /// supported audio formats, and sends processed events through the channel.
    fn handle_raw_event(res: Result<Event, Error>, sender: &Sender<ProcessedEvent>) {
        match res {
            Ok(event) => {
                debug!("Raw file system event: {:?}", event);

                // Skip events without paths
                if event.paths.is_empty() {
                    return;
                }

                Self::process_event_paths(&event, sender);
            }
            Err(e) => {
                error!(error = %e, "File system watcher error");
            }
        }
    }

    /// Processes paths for a given event.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to process.
    /// * `sender` - Channel sender for processed events.
    fn process_event_paths(event: &Event, sender: &Sender<ProcessedEvent>) {
        for path in &event.paths {
            // Check logic depending on event kind
            match event.kind {
                // For Create/Modify, we MUST check extensions to avoid processing non-audio files
                Create(_) | Modify(Data(_)) => {
                    Self::handle_create_modify(path, event.kind, sender);
                }
                Modify(Name(mode)) => {
                    Self::handle_rename(path, mode, event, sender);
                }
                Remove(_) => {
                    Self::handle_remove(path, sender);
                }
                Other => {
                    Self::handle_other(path);
                }
                _ => {
                    debug!("Ignoring event kind {:?} for path: {:?}", event.kind, path);
                }
            }
        }
    }

    /// Handles create and modify events.
    ///
    /// # Arguments
    ///
    /// * `path` - The path associated with the event.
    /// * `kind` - The event kind.
    /// * `sender` - Channel sender for processed events.
    fn handle_create_modify(path: &Path, kind: EventKind, sender: &Sender<ProcessedEvent>) {
        let is_create = matches!(kind, Create(_));

        if path.is_dir() && is_create {
            debug!("FileWatcher: Detected new directory, scanning: {:?}", path);
            let files = Self::collect_audio_files_recursively(path);
            for file_path in files {
                Self::send_file_changed(sender, &file_path, true);
            }
        } else if Self::is_supported_audio_file(path) || Self::is_supported_text_file(path) {
            // Text files might contain DR values, so treat them as file changes
            // This will trigger DR parsing for the parent album directory
            Self::send_file_changed(sender, path, is_create);
        } else {
            debug!("Ignoring unsupported file change: {:?}", path);
        }
    }

    /// Handles rename/move events.
    ///
    /// # Arguments
    ///
    /// * `path` - The path associated with the event.
    /// * `mode` - The rename mode.
    /// * `event` - The full event (needed for Both mode).
    /// * `sender` - Channel sender for processed events.
    fn handle_rename(
        path: &Path,
        mode: RenameMode,
        event: &Event,
        sender: &Sender<ProcessedEvent>,
    ) {
        debug!(
            "FileWatcher: Processing rename event {:?} for path: {:?}",
            mode, path
        );

        match mode {
            // From: File was moved FROM this path (deletion/move source)
            From => {
                Self::handle_rename_from(path, sender);
            }
            To => {
                Self::handle_rename_to(path, sender);
            }
            Both => {
                Self::handle_rename_both(event, sender);
            }
            _ => {
                debug!(
                    "FileWatcher: Ignored unknown RenameMode for path: {:?}",
                    path
                );
            }
        }
    }

    /// Handles rename-from events (file moved from this path).
    ///
    /// # Arguments
    ///
    /// * `path` - The path the file was moved from.
    /// * `sender` - Channel sender for processed events.
    fn handle_rename_from(path: &Path, sender: &Sender<ProcessedEvent>) {
        debug!(
            "FileWatcher: Propagating rename-from (remove) event for path: {:?}",
            path
        );
        Self::send_file_removed(sender, path);
    }

    /// Handles rename-to events (file moved to this path).
    ///
    /// # Arguments
    ///
    /// * `path` - The path the file was moved to.
    /// * `sender` - Channel sender for processed events.
    fn handle_rename_to(path: &Path, sender: &Sender<ProcessedEvent>) {
        if path.is_dir() {
            debug!(
                "FileWatcher: Detected moved directory, scanning: {:?}",
                path
            );
            let files = Self::collect_audio_files_recursively(path);
            for file_path in files {
                Self::send_file_changed(sender, &file_path, true);
            }
        } else if Self::is_supported_audio_file(path) || Self::is_supported_text_file(path) {
            debug!(
                "FileWatcher: Propagating rename-to (add) event for path: {:?}",
                path
            );
            Self::send_file_changed(sender, path, true);
        }
    }

    /// Handles rename-both events (atomic rename with source and destination).
    ///
    /// # Arguments
    ///
    /// * `event` - The full event containing both paths.
    /// * `sender` - Channel sender for processed events.
    fn handle_rename_both(event: &Event, sender: &Sender<ProcessedEvent>) {
        // In Notify, Both usually comes with 2 paths in the event paths vector.
        // If we have 2 paths, 0 is From, 1 is To.
        if event.paths.len() == 2 {
            let from_path = &event.paths[0];
            let to_path = &event.paths[1];

            debug!(
                "FileWatcher: Propagating rename-both: {:?} -> {:?}",
                from_path, to_path
            );

            // Handle From (removal)
            Self::send_file_removed(sender, from_path);

            // Handle To (addition)
            if to_path.is_dir() {
                debug!(
                    "FileWatcher: Detected moved directory (both), scanning: {:?}",
                    to_path
                );
                let files = Self::collect_audio_files_recursively(to_path);
                for file_path in files {
                    Self::send_file_changed(sender, &file_path, true);
                }
            } else if Self::is_supported_audio_file(to_path)
                || Self::is_supported_text_file(to_path)
            {
                Self::send_file_changed(sender, to_path, true);
            }
        } else {
            // Fallback if structure is unexpected
            debug!(
                "FileWatcher: Received RenameMode::Both but path count is {}",
                event.paths.len()
            );
        }
    }

    /// Handles remove events.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to remove.
    /// * `sender` - Channel sender for processed events.
    fn handle_remove(path: &Path, sender: &Sender<ProcessedEvent>) {
        debug!("FileWatcher: Propagating remove event for path: {:?}", path);
        Self::send_file_removed(sender, path);
    }

    /// Handles other event types.
    ///
    /// # Arguments
    ///
    /// * `path` - The path associated with the event.
    fn handle_other(path: &Path) {
        if Self::is_supported_audio_file(path) || Self::is_supported_text_file(path) {
            debug!("Other event kind for path: {:?}", path);
        }
    }

    /// Sends a `FileChanged` event.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel sender for processed events.
    /// * `path` - The path that changed.
    /// * `is_new` - Whether the file is newly created.
    fn send_file_changed(sender: &Sender<ProcessedEvent>, path: &Path, is_new: bool) {
        if let Err(e) = sender.try_send(FileChanged {
            path: path.to_path_buf(),
            is_new,
        }) {
            error!(
                "Failed to send FileChanged event for '{}': {}",
                path.display(),
                e
            );
        }
    }

    /// Sends a `FileRemoved` event.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel sender for processed events.
    /// * `path` - The path that was removed.
    fn send_file_removed(sender: &Sender<ProcessedEvent>, path: &Path) {
        if let Err(e) = sender.try_send(FileRemoved {
            path: path.to_path_buf(),
        }) {
            error!(
                "Failed to send FileRemoved event for '{}': {}",
                path.display(),
                e
            );
        }
    }

    /// Recursively collects audio files from a directory.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - The directory path to search.
    ///
    /// # Returns
    ///
    /// A vector of paths to all supported audio files found recursively.
    fn collect_audio_files_recursively(dir_path: &Path) -> Vec<PathBuf> {
        let mut audio_files = Vec::new();

        if let Ok(entries) = read_dir(dir_path) {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();

                        if path.is_file() {
                            if Self::is_supported_audio_file(&path) {
                                audio_files.push(path);
                            }
                        } else if path.is_dir() {
                            let sub_files = Self::collect_audio_files_recursively(&path);
                            audio_files.extend(sub_files);
                        }
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            ?dir_path,
                            "Failed to read directory entry",
                        );
                    }
                }
            }
        }

        audio_files
    }

    /// Checks if a path corresponds to a supported audio file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check.
    ///
    /// # Returns
    ///
    /// `true` if the path is a supported audio file, `false` otherwise.
    #[must_use]
    pub fn is_supported_audio_file(path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                supported_audio_extensions()
                    .iter()
                    .any(|&ext| ext.eq_ignore_ascii_case(ext_str))
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Checks if a path corresponds to a supported text file for DR value monitoring.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check.
    ///
    /// # Returns
    ///
    /// `true` if the path is a supported text file, `false` otherwise.
    #[must_use]
    pub fn is_supported_text_file(path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                SUPPORTED_TEXT_EXTENSIONS
                    .iter()
                    .any(|&ext| ext.eq_ignore_ascii_case(ext_str))
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Adds a directory to be watched.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path to watch.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the directory cannot be watched.
    pub fn watch_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<(), LibraryError> {
        let path = path.as_ref();

        // Add to watched paths set
        self.watched_paths.write().insert(path.to_path_buf());

        // Start watching the directory recursively
        self.watcher
            .watch(path, Recursive)
            .map_err(|e| LibraryError::InvalidData {
                reason: format!("Failed to watch directory {}: {e}", path.display()),
            })?;

        debug!("Started watching directory: {:?}", path);
        Ok(())
    }

    /// Stops watching a directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path to stop watching.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the directory cannot be unwatched.
    pub fn unwatch_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<(), LibraryError> {
        let path = path.as_ref();

        // Remove from watched paths set
        self.watched_paths.write().remove(path);

        // Stop watching the directory
        self.watcher
            .unwatch(path)
            .map_err(|e| LibraryError::InvalidData {
                reason: format!("Failed to unwatch directory {}: {e}", path.display()),
            })?;

        debug!("Stopped watching directory: {:?}", path);
        Ok(())
    }

    /// Gets the current configuration.
    ///
    /// # Returns
    ///
    /// A reference to the current `FileWatcherConfig`.
    #[must_use]
    pub fn config(&self) -> &FileWatcherConfig {
        &self.config
    }

    /// Gets the set of currently watched paths.
    ///
    /// # Returns
    ///
    /// A reference to the watched paths set.
    #[must_use]
    pub fn watched_paths(&self) -> &Arc<RwLock<HashSet<PathBuf>>> {
        &self.watched_paths
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::library::file_watcher::FileWatcher;

    #[test]
    fn test_supported_audio_extensions() {
        let test_cases = vec![
            // Supported extensions (should return true)
            ("test.flac", true),
            ("test.mp3", true),
            ("test.m4a", true),
            ("test.aac", true),
            ("test.opus", true),
            ("test.ogg", true),
            ("test.wav", true),
            ("test.aiff", true),
            ("test.aif", true),
            ("test.dsf", true),
            ("test.dff", true),
            // Unsupported extensions (should return false)
            ("test.txt", false),
            ("test.mpc", false), // Musepack is not supported by format detector
            ("test", false),
            ("TEST.FLAC", true), // Case insensitive
            ("TEST.M4A", true),  // Case insensitive
        ];

        for (filename, expected) in test_cases {
            let path = PathBuf::from(filename);
            assert_eq!(
                FileWatcher::is_supported_audio_file(&path),
                expected,
                "Failed for filename: {filename}"
            );
        }
    }

    #[test]
    fn test_supported_text_extensions() {
        let test_cases = vec![
            ("test.txt", true),
            ("test.log", true),
            ("test.md", true),
            ("test.csv", true),
            ("test.flac", false),
            ("test", false),
            ("TEST.TXT", true),          // Case insensitive
            ("2012–2017_log.txt", true), // Irregular filename from requirements
        ];

        for (filename, expected) in test_cases {
            let path = PathBuf::from(filename);
            assert_eq!(
                FileWatcher::is_supported_text_file(&path),
                expected,
                "Failed for filename: {filename}"
            );
        }
    }
}

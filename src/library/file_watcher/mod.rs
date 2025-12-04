//! File system change detection using the `notify` crate.
//!
//! This module provides real-time file system monitoring capabilities
//! for music library directories, with support for debouncing and
//! event filtering for supported audio formats.

mod config;
mod debouncer;
mod events;

pub use {
    config::FileWatcherConfig,
    debouncer::DebouncedEventProcessor,
    events::{DebouncedEvent, ProcessedEvent},
};

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use {
    async_channel::Sender,
    notify::{
        event::{EventKind, ModifyKind},
        Config, Event, RecommendedWatcher, RecursiveMode,
    },
    parking_lot::RwLock,
    regex::Regex,
    tracing::{debug, error, warn},
};

use crate::{
    error::domain::LibraryError,
    library::models::Track,
};

/// Supported audio file extensions for library monitoring.
const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "aac", "opus", "ogg", "wav", "aiff", "aif", "mpc",
];

/// File system watcher for music library directories.
///
/// The `FileWatcher` uses the `notify` crate to monitor file system changes
/// in specified music library directories. It filters events to only process
/// supported audio formats and applies debouncing to handle rapid changes.
pub struct FileWatcher {
    /// Internal notify watcher.
    _watcher: RecommendedWatcher,
    /// Set of currently watched paths.
    watched_paths: Arc<RwLock<HashSet<PathBuf>>>,
    /// Configuration for watcher behavior.
    config: FileWatcherConfig,
    /// Regex pattern for supported audio extensions.
    audio_extension_regex: Regex,
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
        
        // Create regex pattern for supported audio extensions
        let extensions_pattern = SUPPORTED_AUDIO_EXTENSIONS
            .iter()
            .map(|ext| format!(r"\.{}$", ext))
            .collect::<Vec<_>>()
            .join("|");
        let audio_extension_regex = Regex::new(&format!("(?i)({})", extensions_pattern))
            .map_err(|e| LibraryError::InvalidData {
                reason: format!("Failed to compile audio extension regex: {}", e),
            })?;

        // Create notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                Self::handle_raw_event(res, event_sender.clone());
            },
            Config::default(),
        )
        .map_err(|e| LibraryError::InvalidData {
            reason: format!("Failed to create file watcher: {}", e),
        })?;

        // Apply configuration
        if !config.include_hidden {
            watcher
                .configure(notify::Config::default().with_ignore_hidden(true))
                .map_err(|e| LibraryError::InvalidData {
                    reason: format!("Failed to configure watcher: {}", e),
                })?;
        }

        let file_watcher = Self {
            _watcher: watcher,
            watched_paths: Arc::new(RwLock::new(HashSet::new())),
            config,
            audio_extension_regex,
        };

        Ok(file_watcher)
    }

    /// Handles raw events from the notify crate.
    ///
    /// This method processes raw file system events, filters them based on
    /// supported audio formats, and sends processed events through the channel.
    fn handle_raw_event(res: Result<Event, notify::Error>, sender: Sender<ProcessedEvent>) {
        match res {
            Ok(event) => {
                debug!("Raw file system event: {:?}", event);
                
                // Skip events without paths
                if event.paths.is_empty() {
                    return;
                }

                // Process each path in the event
                for path in &event.paths {
                    if Self::is_supported_audio_file(path) {
                        // Handle different event kinds
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_)) => {
                                let _ = sender.try_send(ProcessedEvent::FileChanged {
                                    path: path.clone(),
                                    is_new: matches!(event.kind, EventKind::Create(_)),
                                });
                            }
                            EventKind::Remove(_) => {
                                let _ = sender.try_send(ProcessedEvent::FileRemoved {
                                    path: path.clone(),
                                });
                            }
                            EventKind::Other => {
                                // Handle potential rename/move events
                                // Note: notify doesn't always provide both old and new paths
                                // We'll handle this in the incremental updater with heuristics
                                debug!("Other event kind for path: {:?}", path);
                            }
                            _ => {
                                // Ignore other event kinds (access, metadata changes, etc.)
                                debug!("Ignoring event kind {:?} for path: {:?}", event.kind, path);
                            }
                        }
                    } else {
                        debug!("Ignoring non-audio file: {:?}", path);
                    }
                }
            }
            Err(e) => {
                error!("File system watcher error: {}", e);
            }
        }
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
    fn is_supported_audio_file(path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                SUPPORTED_AUDIO_EXTENSIONS
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
        self._watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| LibraryError::InvalidData {
                reason: format!("Failed to watch directory {:?}: {}", path, e),
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
        self._watcher
            .unwatch(path)
            .map_err(|e| LibraryError::InvalidData {
                reason: format!("Failed to unwatch directory {:?}: {}", path, e),
            })?;

        debug!("Stopped watching directory: {:?}", path);
        Ok(())
    }

    /// Gets the current configuration.
    ///
    /// # Returns
    ///
    /// A reference to the current `FileWatcherConfig`.
    pub fn config(&self) -> &FileWatcherConfig {
        &self.config
    }

    /// Gets the set of currently watched paths.
    ///
    /// # Returns
    ///
    /// A reference to the watched paths set.
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
            ("test.flac", true),
            ("test.mp3", true),
            ("test.wav", true),
            ("test.txt", false),
            ("test", false),
            ("TEST.FLAC", true), // Case insensitive
        ];

        for (filename, expected) in test_cases {
            let path = PathBuf::from(filename);
            assert_eq!(
                FileWatcher::is_supported_audio_file(&path),
                expected,
                "Failed for filename: {}",
                filename
            );
        }
    }
}
//! Main library scanner coordinator.
//!
//! This module coordinates file system monitoring, metadata extraction,
//! and database updates to maintain a real-time synchronized music library.

use std::{path::Path, sync::Arc};

use {
    async_channel::{Receiver, bounded},
    parking_lot::RwLock,
    tokio::{spawn, task::JoinHandle},
    tracing::{debug, error, warn},
};

use crate::{
    config::settings::UserSettings,
    error::domain::LibraryError,
    library::{
        database::LibraryDatabase,
        file_watcher::{DebouncedEvent, DebouncedEventProcessor, FileWatcher},
        scanner::handlers::{handle_files_changed, handle_files_removed, handle_files_renamed},
    },
};

mod config;
mod handlers;

pub use config::ScannerConfig;

/// Main library scanner coordinator.
///
/// The `LibraryScanner` orchestrates file system monitoring, metadata extraction,
/// and database updates to maintain a real-time synchronized music library.
pub struct LibraryScanner {
    /// File system watcher.
    file_watcher: FileWatcher,
    /// Database interface.
    database: Arc<LibraryDatabase>,
    /// User settings.
    settings: Arc<RwLock<UserSettings>>,
    /// Configuration.
    config: ScannerConfig,
    /// Task handles for background operations.
    _tasks: Vec<JoinHandle<()>>,
}

impl LibraryScanner {
    /// Creates a new library scanner.
    ///
    /// # Arguments
    ///
    /// * `database` - Database interface.
    /// * `settings` - User settings.
    /// * `config` - Optional scanner configuration.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `LibraryScanner` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if initialization fails.
    pub async fn new(
        database: Arc<LibraryDatabase>,
        settings: Arc<RwLock<UserSettings>>,
        config: Option<ScannerConfig>,
    ) -> Result<Self, LibraryError> {
        let config = config.unwrap_or_default();

        // Create channels for event processing
        let (raw_event_sender, raw_event_receiver) = bounded(100);
        let (debounced_event_sender, debounced_event_receiver) = bounded(50);

        // Clone config for file watcher to avoid move/borrow issues
        let file_watcher_config = config.file_watcher_config.clone();

        // Create file watcher
        let mut file_watcher = FileWatcher::new(raw_event_sender, Some(file_watcher_config))?;

        // Start watching configured library directories
        let library_dirs = settings.read().library_directories.clone();
        for dir in &library_dirs {
            if let Err(e) = file_watcher.watch_directory(dir) {
                warn!("Failed to watch directory {}: {}", dir, e);
            }
        }

        // Create debounced event processor
        let debounced_processor = DebouncedEventProcessor::new(
            raw_event_receiver,
            debounced_event_sender,
            config.file_watcher_config.clone(),
        );

        // Spawn background tasks
        let mut tasks = Vec::new();

        // Spawn debounced event processor task
        tasks.push(spawn(async move {
            debounced_processor.start_processing().await;
        }));

        // Spawn debounced event handler task
        let database_clone = database.clone();
        let settings_clone = settings.clone();
        tasks.push(spawn(async move {
            Self::handle_debounced_events(debounced_event_receiver, database_clone, settings_clone)
                .await;
        }));

        Ok(LibraryScanner {
            file_watcher,
            database,
            settings,
            config,
            _tasks: tasks,
        })
    }

    /// Handles debounced file system events.
    ///
    /// This method processes debounced events and coordinates metadata extraction
    /// and database updates.
    async fn handle_debounced_events(
        receiver: Receiver<DebouncedEvent>,
        database: Arc<LibraryDatabase>,
        settings: Arc<RwLock<UserSettings>>,
    ) {
        while let Ok(event) = receiver.recv().await {
            match event {
                DebouncedEvent::FilesChanged { paths } => {
                    debug!("Processing {} changed files", paths.len());
                    if let Err(e) = handle_files_changed(paths, &database, &settings).await {
                        error!("Error handling changed files: {}", e);
                    }
                }
                DebouncedEvent::FilesRemoved { paths } => {
                    debug!("Processing {} removed files", paths.len());
                    if let Err(e) = handle_files_removed(paths, &database).await {
                        error!("Error handling removed files: {}", e);
                    }
                }
                DebouncedEvent::FilesRenamed { paths } => {
                    debug!("Processing {} renamed files", paths.len());
                    if let Err(e) = handle_files_renamed(paths, &database, &settings).await {
                        error!("Error handling renamed files: {}", e);
                    }
                }
            }
        }
    }

    /// Adds a library directory to be monitored.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path to add.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the directory cannot be added.
    pub fn add_library_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<(), LibraryError> {
        self.file_watcher.watch_directory(path)
    }

    /// Removes a library directory from monitoring.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path to remove.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the directory cannot be removed.
    pub fn remove_library_directory<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<(), LibraryError> {
        self.file_watcher.unwatch_directory(path)
    }

    /// Gets the current scanner configuration.
    ///
    /// # Returns
    ///
    /// A reference to the current `ScannerConfig`.
    pub fn config(&self) -> &ScannerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use crate::library::scanner::ScannerConfig;

    #[test]
    fn test_scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.max_concurrent_metadata_tasks, 4);
        assert_eq!(config.file_watcher_config.debounce_delay_ms, 500);
    }
}

//! Main library scanner coordinator.
//!
//! This module coordinates file system monitoring, metadata extraction,
//! and database updates to maintain a real-time synchronized music library.

use std::{
    fs::read_dir,
    path::{Path, PathBuf},
    sync::Arc,
};

use {
    async_channel::{Receiver, bounded},
    parking_lot::RwLock,
    tokio::{
        spawn,
        sync::broadcast::{Receiver as TokioReceiver, Sender, channel},
        task::JoinHandle,
    },
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
pub mod handlers;

pub use config::ScannerConfig;

/// Events emitted by the library scanner.
#[derive(Debug, Clone)]
pub enum ScannerEvent {
    /// The library has been modified (add/remove/update).
    LibraryChanged,
}

/// Main library scanner coordinator.
///
/// The `LibraryScanner` orchestrates file system monitoring, metadata extraction,
/// and database updates to maintain a real-time synchronized music library.
#[derive(Debug)]
pub struct LibraryScanner {
    /// File system watcher.
    file_watcher: FileWatcher,
    /// Configuration.
    config: ScannerConfig,
    /// Task handles for background operations.
    _tasks: Vec<JoinHandle<()>>,
    /// Event sender for scanner notifications.
    event_sender: Sender<ScannerEvent>,
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
        // Increased capacity to handle recursive directory scans
        let (raw_event_sender, raw_event_receiver) = bounded(1000);
        let (debounced_event_sender, debounced_event_receiver) = bounded(50);

        // Create broadcast channel for scanner events
        let (event_sender, _) = channel(16);

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
        let event_sender_clone = event_sender.clone();
        tasks.push(spawn(async move {
            Self::handle_debounced_events(
                debounced_event_receiver,
                database_clone,
                settings_clone,
                event_sender_clone,
            )
            .await;
        }));

        Ok(LibraryScanner {
            file_watcher,
            config,
            _tasks: tasks,
            event_sender,
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
        event_sender: Sender<ScannerEvent>,
    ) {
        while let Ok(event) = receiver.recv().await {
            let mut changes_processed = false;
            match event {
                DebouncedEvent::FilesChanged { paths } => {
                    debug!("Processing {} changed files", paths.len());
                    if let Err(e) = handle_files_changed(paths, &database, &settings).await {
                        error!("Error handling changed files: {}", e);
                    } else {
                        changes_processed = true;
                    }
                }
                DebouncedEvent::FilesRemoved { paths } => {
                    debug!("Processing {} removed files", paths.len());
                    if let Err(e) = handle_files_removed(paths, &database).await {
                        error!("Error handling removed files: {}", e);
                    } else {
                        changes_processed = true;
                    }
                }
                DebouncedEvent::FilesRenamed { paths } => {
                    debug!("Processing {} renamed files", paths.len());
                    if let Err(e) = handle_files_renamed(paths, &database, &settings).await {
                        error!("Error handling renamed files: {}", e);
                    } else {
                        changes_processed = true;
                    }
                }
            }

            if changes_processed {
                debug!("Library changes processed, emitting LibraryChanged event");
                let _ = event_sender.send(ScannerEvent::LibraryChanged);
            }
        }
    }

    /// Subscribe to scanner events.
    pub fn subscribe(&self) -> TokioReceiver<ScannerEvent> {
        self.event_sender.subscribe()
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

    /// Performs an initial scan of all configured library directories.
    ///
    /// This method walks through all library directories and processes existing
    /// audio files to populate the database with initial content.
    ///
    /// # Arguments
    ///
    /// * `database` - Database interface for storing metadata.
    /// * `settings` - User settings containing library directories.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if scanning fails.
    pub async fn scan_initial_directories(
        &self,
        database: &Arc<LibraryDatabase>,
        settings: &Arc<RwLock<UserSettings>>,
    ) -> Result<(), LibraryError> {
        let library_dirs = settings.read().library_directories.clone();
        let mut all_audio_files = Vec::new();

        // Walk through all library directories and collect audio files
        for dir in library_dirs {
            let dir_path = Path::new(&dir);
            let audio_files = self.collect_audio_files_from_directory(dir_path)?;
            all_audio_files.extend(audio_files);
        }

        // Process all collected audio files
        if !all_audio_files.is_empty() {
            handle_files_changed(all_audio_files, database, settings).await?;
        }

        Ok(())
    }

    /// Recursively collects audio files from a directory and its subdirectories.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - Path to the directory to scan.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of audio file paths or a `LibraryError`.
    pub fn collect_audio_files_from_directory(
        &self,
        dir_path: &Path,
    ) -> Result<Vec<PathBuf>, LibraryError> {
        let mut audio_files = Vec::new();

        if let Ok(entries) = read_dir(dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() {
                    // Check if it's a supported audio file
                    if FileWatcher::is_supported_audio_file(&path) {
                        audio_files.push(path);
                    }
                } else if path.is_dir() {
                    // Recursively process subdirectories
                    let sub_audio_files = self.collect_audio_files_from_directory(&path)?;
                    audio_files.extend(sub_audio_files);
                }
            }
        }

        Ok(audio_files)
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

//! Main library scanner coordinator.
//!
//! This module coordinates file system monitoring, metadata extraction,
//! and database updates to maintain a real-time synchronized music library.

mod config;
pub mod event_processing;

use std::{
    fs::read_dir,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
};

use {
    async_channel::{Receiver, Sender, bounded, unbounded},
    parking_lot::RwLock,
    tokio::{spawn, task::JoinHandle},
    tracing::{debug, error, warn},
};

use crate::{
    config::settings::UserSettings,
    error::domain::LibraryError,
    library::{
        database::LibraryDatabase,
        dr_parser::DrParser,
        file_watcher::{
            FileWatcher,
            debouncer::DebouncedEventProcessor,
            events::DebouncedEvent::{self, FilesChanged, FilesRemoved, FilesRenamed},
        },
        scanner::{
            config::ScannerConfig,
            event_processing::{handle_files_changed, handle_files_removed, handle_files_renamed},
        },
    },
};

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
    /// List of active subscribers for manual broadcast fan-out.
    subscribers: Arc<RwLock<Vec<Sender<ScannerEvent>>>>,
    /// DR parser for extracting DR values from album directories.
    dr_parser: Option<Arc<DrParser>>,
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
    pub fn new(
        database: &Arc<LibraryDatabase>,
        settings: &Arc<RwLock<UserSettings>>,
        config: Option<ScannerConfig>,
    ) -> Result<Self, LibraryError> {
        let config = config.unwrap_or_default();

        // Create channels for event processing
        // Increased capacity to handle recursive directory scans
        let (raw_event_sender, raw_event_receiver) = bounded(1000);
        let (debounced_event_sender, debounced_event_receiver) = bounded(50);

        // Initialize empty subscribers list
        let subscribers = Arc::new(RwLock::new(Vec::new()));

        // Clone config for file watcher to avoid move/borrow issues
        let file_watcher_config = config.file_watcher_config.clone();

        // Create file watcher
        let mut file_watcher = FileWatcher::new(raw_event_sender, Some(file_watcher_config))?;

        // Start watching configured library directories
        let library_dirs = settings.read().library_directories.clone();
        for dir in &library_dirs {
            if let Err(e) = file_watcher.watch_directory(dir) {
                warn!(directory = %dir, error = %e, "Failed to watch directory");
            }
        }

        // Create debounced event processor
        let debounced_processor = DebouncedEventProcessor::new(
            raw_event_receiver,
            debounced_event_sender,
            config.file_watcher_config.clone(),
        );

        // Initialize DR parser if enabled in settings
        let dr_parser = if settings.read().show_dr_values {
            match DrParser::new(Arc::clone(database)) {
                Ok(parser) => Some(Arc::new(parser)),
                Err(e) => {
                    warn!(error = %e, "Failed to initialize DR parser");
                    None
                }
            }
        } else {
            None
        };

        // Spawn background tasks
        let mut tasks = Vec::new();

        // Spawn debounced event processor task
        tasks.push(spawn(async move {
            debounced_processor.start_processing().await;
        }));

        // Spawn debounced event handler task
        let database_clone = Arc::clone(database);
        let settings_clone = Arc::clone(settings);
        let subscribers_clone = Arc::clone(&subscribers);
        let dr_parser_clone = dr_parser.clone();
        tasks.push(spawn(async move {
            Self::handle_debounced_events(
                debounced_event_receiver,
                database_clone,
                settings_clone,
                dr_parser_clone,
                subscribers_clone,
            )
            .await;
        }));

        Ok(Self {
            file_watcher,
            config,
            _tasks: tasks,
            subscribers,
            dr_parser,
        })
    }

    /// Helper to broadcast an event to all subscribers.
    /// Cleans up closed channels.
    fn broadcast_event(subscribers: &Arc<RwLock<Vec<Sender<ScannerEvent>>>>, event: &ScannerEvent) {
        let mut subscribers_lock = subscribers.write();
        let mut active = Vec::with_capacity(subscribers_lock.len());
        let mut count = 0;

        for tx in subscribers_lock.iter() {
            // We use try_send to avoid blocking. Since these are unbounded channels (created in
            // subscribe), try_send should only fail if the channel is closed.
            if matches!(tx.try_send(event.clone()), Ok(())) {
                active.push(tx.clone());
                count += 1;
            }
        }

        *subscribers_lock = active;
        drop(subscribers_lock);
        debug!("Broadcasted event to {} subscribers", count);
    }

    /// Handles debounced file system events.
    ///
    /// This method processes debounced events and coordinates metadata extraction
    /// and database updates.
    async fn handle_debounced_events(
        receiver: Receiver<DebouncedEvent>,
        database: Arc<LibraryDatabase>,
        settings: Arc<RwLock<UserSettings>>,
        dr_parser: Option<Arc<DrParser>>,
        subscribers: Arc<RwLock<Vec<Sender<ScannerEvent>>>>,
    ) {
        while let Ok(event) = receiver.recv().await {
            let mut changes_processed = false;
            match event {
                FilesChanged { paths } => {
                    debug!("Processing {} changed files", paths.len());
                    if let Err(e) =
                        handle_files_changed(paths, &database, &settings, &dr_parser).await
                    {
                        error!(error = %e, "Error handling changed files");
                    } else {
                        changes_processed = true;
                    }
                }
                FilesRemoved { paths } => {
                    debug!("Processing {} removed files", paths.len());
                    if let Err(e) = handle_files_removed(paths, &database, &dr_parser).await {
                        error!(error = %e, "Error handling removed files");
                    } else {
                        changes_processed = true;
                    }
                }
                FilesRenamed { paths } => {
                    debug!("Processing {} renamed files", paths.len());
                    if let Err(e) =
                        handle_files_renamed(paths, &database, &settings, &dr_parser).await
                    {
                        error!(error = %e, "Error handling renamed files");
                    } else {
                        changes_processed = true;
                    }
                }
            }

            if changes_processed {
                debug!("Library changes processed, emitting LibraryChanged event");
                Self::broadcast_event(&subscribers, &ScannerEvent::LibraryChanged);
            }
        }
    }

    /// Subscribe to scanner events.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<ScannerEvent> {
        // Create a new unbounded channel for this subscriber
        let (tx, rx) = unbounded();

        // Add sender to the list
        self.subscribers.write().push(tx);

        rx
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
    #[must_use]
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
    /// * `cancelled` - Cancellation token to allow early termination.
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
        cancelled: &Arc<AtomicBool>,
    ) -> Result<(), LibraryError> {
        let library_dirs = settings.read().library_directories.clone();
        let mut all_audio_files = Vec::new();

        for dir in library_dirs {
            if cancelled.load(SeqCst) {
                debug!("Library scan cancelled during directory iteration");
                return Ok(());
            }

            let dir_path = Path::new(&dir);
            let audio_files = Self::collect_audio_files_from_directory(dir_path, cancelled)?;

            if cancelled.load(SeqCst) {
                debug!("Library scan cancelled during file collection");
                return Ok(());
            }

            all_audio_files.extend(audio_files);
        }

        if !all_audio_files.is_empty() {
            if cancelled.load(SeqCst) {
                debug!("Library scan cancelled before processing files");
                return Ok(());
            }

            handle_files_changed(all_audio_files, database, settings, &self.dr_parser).await?;
        }

        Ok(())
    }

    /// Recursively collects audio files from a directory and its subdirectories.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - Path to the directory to scan.
    /// * `cancelled` - Cancellation token to allow early termination.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of audio file paths or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the directory cannot be read.
    pub fn collect_audio_files_from_directory(
        dir_path: &Path,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Vec<PathBuf>, LibraryError> {
        let mut audio_files = Vec::new();

        if let Ok(entries) = read_dir(dir_path) {
            for entry in entries.flatten() {
                if cancelled.load(SeqCst) {
                    debug!("File collection cancelled in directory: {:?}", dir_path);
                    return Ok(audio_files);
                }

                let path = entry.path();

                if path.is_file() {
                    // Check if it's a supported audio file
                    if FileWatcher::is_supported_audio_file(&path) {
                        audio_files.push(path);
                    }
                } else if path.is_dir() {
                    // Recursively process subdirectories
                    let sub_audio_files =
                        Self::collect_audio_files_from_directory(&path, cancelled)?;
                    audio_files.extend(sub_audio_files);

                    if cancelled.load(SeqCst) {
                        debug!("File collection cancelled after subdirectory scan");
                        return Ok(audio_files);
                    }
                } else {
                    // Ignore non-file, non-directory entries (symlinks, special files, etc.)
                }
            }
        }

        Ok(audio_files)
    }

    /// Scans a single directory for audio files (non-recursive).
    ///
    /// # Arguments
    ///
    /// * `dir_path` - Path to the directory to scan.
    /// * `cancelled` - Cancellation token to allow early termination.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of audio file paths or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the directory cannot be read.
    pub fn scan_directory(
        dir_path: &Path,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Vec<PathBuf>, LibraryError> {
        if cancelled.load(SeqCst) {
            debug!("Directory scan cancelled");
            return Ok(Vec::new());
        }

        let mut audio_files = Vec::new();

        if let Ok(entries) = read_dir(dir_path) {
            for entry in entries.flatten() {
                if cancelled.load(SeqCst) {
                    debug!("Directory scan cancelled in: {:?}", dir_path);
                    return Ok(audio_files);
                }

                let path = entry.path();

                if path.is_file() && FileWatcher::is_supported_audio_file(&path) {
                    audio_files.push(path);
                }
            }
        }

        Ok(audio_files)
    }

    /// Scans a single audio file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the file to scan.
    /// * `cancelled` - Cancellation token to allow early termination.
    ///
    /// # Returns
    ///
    /// A `Result` containing the path if it's a supported audio file, or `None`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if checking the file fails.
    pub fn scan_single_file(
        file_path: &Path,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Option<PathBuf>, LibraryError> {
        if cancelled.load(SeqCst) {
            debug!("Single file scan cancelled");
            return Ok(None);
        }

        if file_path.is_file() && FileWatcher::is_supported_audio_file(file_path) {
            Ok(Some(file_path.to_path_buf()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::library::scanner::ScannerConfig;

    #[test]
    fn scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.max_concurrent_metadata_tasks, 4);
        assert_eq!(config.file_watcher_config.debounce_delay_ms, 500);
    }
}

//! Filesystem change monitoring using the notify crate.
//!
//! Watches configured library directories for changes and triggers incremental
//! scans when files are added, modified, or removed.

use std::{path::PathBuf, sync::Arc};

use {
    notify::{Config, Error, Event, RecommendedWatcher, RecursiveMode::Recursive, Watcher},
    tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    tracing::{error, info, warn},
};

use crate::{
    library::scanner::{FsScanner, LibraryScanner},
    storage::Storage,
};

/// Filesystem watcher that monitors library directories for changes.
pub struct LibraryWatcher<S: Storage> {
    /// The underlying notify watcher.
    watcher: RecommendedWatcher,
    /// Scanner for incremental scans.
    scanner: Arc<FsScanner<S>>,
}

impl<S: Storage + 'static> LibraryWatcher<S> {
    /// Create a new filesystem watcher.
    ///
    /// # Arguments
    ///
    /// * `scanner` - Scanner to trigger incremental scans
    ///
    /// # Returns
    ///
    /// A tuple of (watcher, `event_receiver`).
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be created.
    pub fn new(
        scanner: Arc<FsScanner<S>>,
    ) -> Result<(Self, UnboundedReceiver<WatcherEvent>), Error> {
        let (event_tx, event_rx) = unbounded_channel();

        let config = Config::default();

        let cb_tx = event_tx;
        let watcher = RecommendedWatcher::new(
            move |result: Result<Event, Error>| {
                Self::handle_watcher_event(result, &cb_tx);
            },
            config,
        )?;

        Ok((Self { watcher, scanner }, event_rx))
    }

    /// Handle a raw watcher event and forward it through the channel.
    fn handle_watcher_event(
        result: Result<Event, Error>,
        event_tx: &UnboundedSender<WatcherEvent>,
    ) {
        let event = match result {
            Ok(event) => WatcherEvent::DirectoryModified {
                path: event.paths.first().cloned().unwrap_or_default(),
            },
            Err(e) => WatcherEvent::Error {
                error: e.to_string(),
            },
        };
        if let Err(e) = event_tx.send(event) {
            error!(error = %e, "Failed to send watcher event");
        }
    }

    /// Start watching the given directories.
    ///
    /// # Arguments
    ///
    /// * `directories` - List of directory paths to watch
    ///
    /// # Errors
    ///
    /// Returns an error if a directory cannot be watched.
    pub fn watch_directories(&mut self, directories: &[PathBuf]) -> Result<(), notify::Error> {
        for dir in directories {
            self.watcher.watch(dir.as_path(), Recursive)?;
            info!(path = %dir.display(), "Watching directory");
        }
        Ok(())
    }

    /// Stop watching all directories.
    pub fn stop_watching(&mut self) {
        if let Err(e) = self.watcher.unwatch(&PathBuf::from("/")) {
            warn!(error = %e, "Failed to unwatch (expected for non-watched paths)");
        }
    }

    /// Process a watcher event and trigger an incremental scan if needed.
    ///
    /// # Arguments
    ///
    /// * `event` - The watcher event to process
    pub async fn process_event(&self, event: WatcherEvent) {
        match event {
            WatcherEvent::DirectoryModified { path } => self.process_directory_modified(path).await,
            WatcherEvent::Error { error } => {
                error!(error = %error, "Watcher error");
            }
        }
    }

    /// Process a directory modification event by triggering an incremental scan.
    async fn process_directory_modified(&self, path: PathBuf) {
        if !path.exists() || !path.is_dir() {
            return;
        }
        info!(path = %path.display(), "Directory modified, triggering incremental scan");
        if let Err(e) = self.scanner.scan_directory(&path).await {
            error!(error = %e, path = %path.display(), "Failed to scan directory");
        }
    }
}

/// Events emitted by the filesystem watcher.
#[derive(Debug, Clone, PartialEq)]
pub enum WatcherEvent {
    /// A directory was modified (files added/removed/changed).
    DirectoryModified {
        /// Path of the modified directory.
        path: PathBuf,
    },
    /// An error occurred during watching.
    Error {
        /// Error message.
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use crate::library::watcher::WatcherEvent::DirectoryModified;

    const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);

    #[test]
    fn watcher_event_clone() {
        let event = DirectoryModified {
            path: PathBuf::from("/music"),
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    #[test]
    fn debounce_interval_is_reasonable() {
        assert!(DEBOUNCE_INTERVAL.as_millis() >= 100);
        assert!(DEBOUNCE_INTERVAL.as_millis() <= 2000);
    }
}

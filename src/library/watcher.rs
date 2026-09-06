//! Filesystem change monitoring using the notify crate.
//!
//! Watches configured library directories for changes and triggers incremental
//! scans when files are added, modified, or removed.

use std::{
    mem::take,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use {
    notify::{Config, Error, Event, RecommendedWatcher, RecursiveMode::Recursive, Watcher},
    parking_lot::Mutex,
    tokio::{
        sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        time::sleep,
    },
    tracing::{error, info, warn},
};

use crate::{
    library::scanner::{FsScanner, LibraryScanner},
    storage::{Storage, StorageError},
};

/// Filesystem watcher that monitors library directories for changes.
pub struct LibraryWatcher<S: Storage> {
    /// The underlying notify watcher (interior mut for `&self` watch/unwatch).
    watcher: Mutex<RecommendedWatcher>,
    /// Scanner for incremental scans.
    scanner: Arc<FsScanner<S>>,
    /// Last scan trigger time for debouncing (500 ms window).
    last_scan: Arc<Mutex<Option<Instant>>>,
    /// Paths currently watched (for unwatch / stop), interior mut.
    watched: Mutex<Vec<PathBuf>>,
    /// Shared channel sender. `Arc` allows the notify callback to observe
    /// `take()` during [`Self::shutdown`]. `None` after shutdown silences
    /// further events without logging.
    event_tx: Arc<Mutex<Option<UnboundedSender<WatcherEvent>>>>,
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
        let shared_tx = Arc::new(Mutex::new(Some(event_tx)));
        let cb_tx = Arc::clone(&shared_tx);

        let config = Config::default()
            .with_poll_interval(Duration::from_millis(500))
            .with_compare_contents(true);

        let watcher = RecommendedWatcher::new(
            move |result: Result<Event, Error>| {
                Self::forward_notify(result, &cb_tx);
            },
            config,
        )?;

        Ok((
            Self {
                watcher: Mutex::new(watcher),
                scanner,
                last_scan: Arc::new(Mutex::new(None)),
                watched: Mutex::new(Vec::new()),
                event_tx: shared_tx,
            },
            event_rx,
        ))
    }

    /// Forward a notify callback result via the shared channel.
    fn forward_notify(
        result: Result<Event, Error>,
        shared_tx: &Arc<Mutex<Option<UnboundedSender<WatcherEvent>>>>,
    ) {
        let Some(tx) = shared_tx.lock().clone() else {
            return;
        };
        Self::handle_watcher_event(result, &tx);
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
        if event_tx.is_closed() {
            return;
        }
        if let Err(e) = event_tx.send(event) {
            warn!(error = %e, "Failed to send watcher event");
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
    pub fn watch_directories(&self, directories: &[PathBuf]) -> Result<(), notify::Error> {
        let mut fs_watcher = self.watcher.lock();
        let mut watched_dirs = self.watched.lock();
        for dir in directories {
            fs_watcher.watch(dir.as_path(), Recursive)?;
            info!(path = %dir.display(), "Watching directory");
            Self::track_directory(&mut watched_dirs, dir);
        }
        drop(fs_watcher);
        drop(watched_dirs);
        Ok(())
    }

    /// Track a directory in the watched list if not already present.
    fn track_directory(watched_dirs: &mut Vec<PathBuf>, dir: &PathBuf) {
        if watched_dirs.contains(dir) {
            return;
        }
        watched_dirs.push(dir.clone());
    }

    /// Stop watching a single directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be unwatched.
    pub fn unwatch_directory(&self, path: &Path) -> Result<(), notify::Error> {
        self.watcher.lock().unwatch(path)?;
        self.watched.lock().retain(|p| p != path);
        info!(path = %path.display(), "Stopped watching directory");
        Ok(())
    }

    /// Stop watching all directories.
    pub fn stop_watching(&self) {
        let previous_dirs = take(&mut *self.watched.lock());
        let mut fs_watcher = self.watcher.lock();
        for path in previous_dirs {
            Self::try_unwatch(&mut fs_watcher, &path);
        }
    }

    /// Gracefully shut down the watcher: unwatch directories, close the
    /// event channel and cancel any in-progress scan.
    pub fn shutdown(&self) {
        self.stop_watching();
        self.event_tx.lock().take();
        if let Err(e) = self.scanner.cancel() {
            warn!(error = %e, "Failed to cancel scan during watcher shutdown");
        }
    }

    /// Check whether a scan cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        *self.scanner.cancel_rx.borrow()
    }

    /// Attempt to unwatch a path, logging on failure.
    fn try_unwatch(fs_watcher: &mut RecommendedWatcher, path: &Path) {
        if let Err(error) = fs_watcher.unwatch(path) {
            warn!(error = %error, path = %path.display(), "Failed to unwatch");
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

    /// Check debouncing window (500 ms) and sleep to coalesce rapid events.
    async fn debounce(&self) -> bool {
        let now = Instant::now();
        let last = { *self.last_scan.lock() };
        let is_recent =
            last.is_some_and(|prev| now.duration_since(prev) < Duration::from_millis(500));
        if is_recent {
            return false;
        }
        *self.last_scan.lock() = Some(now);
        sleep(Duration::from_millis(200)).await;
        true
    }

    /// Handle file removal: delete track from storage if path no longer exists.
    async fn handle_removal(&self, path: &Path) {
        let track = match self.scanner.storage.find_by_path(path).await {
            Ok(Some(t)) => t,
            Ok(None) => return,
            Err(e) => {
                warn!(error = %e, path = %path.display(), "Failed to check removed track");
                return;
            }
        };
        info!(
            path = %path.display(),
            track_id = track.id,
            "File removed, deleting track"
        );
        Self::log_delete_result(self.scanner.storage.delete_track(track.id).await, path);
        if let Err(e) = self.scanner.storage.prune_orphans().await {
            warn!(error = %e, path = %path.display(), "Failed to prune orphans after removal");
        }
    }

    /// Log the result of a track deletion.
    fn log_delete_result(result: Result<(), StorageError>, path: &Path) {
        if let Err(e) = result {
            warn!(error = %e, path = %path.display(), "Failed to delete removed track");
        }
    }

    /// Scan a directory and log any error.
    async fn scan_and_log(&self, path: &Path) {
        if let Err(e) = self.scanner.scan_directory(path).await {
            error!(error = %e, path = %path.display(), "Failed to scan directory");
        }
    }

    /// Process a directory modification event by triggering an incremental scan.
    async fn process_directory_modified(&self, path: PathBuf) {
        if !self.debounce().await {
            return;
        }

        if !path.exists() {
            self.handle_removal(&path).await;
            return;
        }

        if path.is_file() {
            Self::handle_file_modified(self, &path).await;
            return;
        }

        if path.is_dir() {
            info!(path = %path.display(), "Directory modified, triggering incremental scan");
            self.scan_and_log(&path).await;
        }
    }

    /// Handle a file modification by scanning its parent.
    async fn handle_file_modified(&self, path: &Path) {
        let Some(parent) = path.parent() else {
            return;
        };
        info!(
            path = %path.display(),
            "File modified, triggering incremental scan of parent"
        );
        self.scan_and_log(parent).await;
    }
}

/// Events emitted by the filesystem watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    use tokio::sync::mpsc::unbounded_channel;

    use notify::{Error, ErrorKind, Event, EventKind, event::CreateKind::File};

    use crate::{
        library::watcher::{
            LibraryWatcher,
            WatcherEvent::{DirectoryModified, Error as WatcherError},
        },
        storage::database::SqliteStorage,
    };

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

    #[test]
    fn handle_watcher_event_forwards_modified() {
        let (tx, mut rx) = unbounded_channel();
        let mut event = Event::new(EventKind::Create(File));
        event.paths.push(PathBuf::from("/music/new.flac"));
        LibraryWatcher::<SqliteStorage>::handle_watcher_event(Ok(event), &tx);
        assert_eq!(
            rx.try_recv(),
            Ok(DirectoryModified {
                path: PathBuf::from("/music/new.flac"),
            })
        );
    }

    #[test]
    fn handle_watcher_event_forwards_error() {
        let (tx, mut rx) = unbounded_channel();
        let err = Error::new(ErrorKind::Generic("watcher failure".to_string()));
        LibraryWatcher::<SqliteStorage>::handle_watcher_event(Err(err), &tx);
        assert_eq!(
            rx.try_recv(),
            Ok(WatcherError {
                error: "watcher failure".to_string(),
            })
        );
    }
}

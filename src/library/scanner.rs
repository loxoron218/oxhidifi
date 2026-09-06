//! Filesystem scanner that walks directories and discovers audio files.
//!
//! Implements the [`LibraryScanner`] trait for scanning configured library directories,
//! extracting metadata, deduplicating tracks, and persisting results to storage.

pub mod events;
pub mod fingerprint_check;
pub mod ingest;
pub mod screening;
pub mod tag_resolve;
pub mod timefmt;
pub mod walker;

use std::{future::Future, path::Path, result::Result, sync::Arc};

use {
    async_channel::Sender,
    parking_lot::Mutex,
    tokio::sync::watch::{Receiver, Sender as TokioSender, channel},
};

use crate::{
    library::scanner::events::ScanEvent,
    storage::{Storage, StorageError},
    ui::signal::ValueSignal,
};

/// Filesystem-based library scanner with storage integration.
pub struct FsScanner<S: Storage> {
    /// Storage backend for persistence.
    pub storage: Arc<S>,
    /// Maximum number of concurrent metadata extractions.
    pub max_concurrent: usize,
    /// Cancellation signal sender.
    pub cancel_tx: TokioSender<bool>,
    /// Cancellation signal receiver (cloned into scan tasks).
    pub cancel_rx: Receiver<bool>,
    /// Channel sender for forwarding scan events to the UI.
    pub scan_event_tx: Sender<ScanEvent>,
    /// Optional refresh signal for incremental grid updates (FR-006).
    pub refresh: Mutex<Option<ValueSignal<()>>>,
}

impl<S: Storage> FsScanner<S> {
    /// Create a new filesystem scanner.
    pub fn new(storage: Arc<S>, scan_event_tx: Sender<ScanEvent>, max_concurrent: usize) -> Self {
        let (cancel_tx, cancel_rx) = channel(false);
        Self {
            storage,
            max_concurrent,
            cancel_tx,
            cancel_rx,
            scan_event_tx,
            refresh: Mutex::new(None),
        }
    }

    /// Set the refresh signal for incremental UI updates.
    pub fn set_refresh(&self, refresh: ValueSignal<()>) {
        *self.refresh.lock() = Some(refresh);
    }
}

impl<S: Storage + 'static> LibraryScanner for FsScanner<S> {
    async fn scan_all(&self) -> Result<(), StorageError> {
        let dirs = self.storage.list_library_directories().await?;

        for dir in &dirs {
            let path = Path::new(&dir.path);
            self.scan_dir(path).await;
        }

        Ok(())
    }

    async fn scan_directory(&self, path: &Path) -> Result<(), StorageError> {
        self.scan_dir(path).await;
        Ok(())
    }

    fn cancel(&self) -> Result<(), StorageError> {
        self.cancel_tx
            .send(true)
            .map_err(|e| StorageError::Database(format!("Failed to send cancel signal: {e}")))
    }
}

/// Controls and observes library scanning.
pub trait LibraryScanner: Send + 'static {
    /// Trigger a full scan of all configured directories.
    fn scan_all(&self) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Trigger a scan of a specific directory.
    fn scan_directory(&self, path: &Path) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Cancel any in-progress scan.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the cancellation signal cannot be sent.
    fn cancel(&self) -> Result<(), StorageError>;
}

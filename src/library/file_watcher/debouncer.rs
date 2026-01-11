//! Debounced event processor that handles rapid file changes.

use std::{collections::HashSet, mem::take, path::PathBuf, sync::Arc, time::Duration};

use {
    async_channel::{Receiver, Sender},
    parking_lot::RwLock,
    tokio::{pin, select, time::sleep},
    tracing::error,
};

use crate::library::file_watcher::{
    config::FileWatcherConfig,
    events::{
        DebouncedEvent::{self, FilesChanged, FilesRemoved, FilesRenamed},
        ProcessedEvent::{self, FileChanged, FileRemoved, FileRenamed},
    },
};

/// Debounced event processor that handles rapid file changes.
///
/// This struct implements debouncing logic to prevent processing
/// multiple events for the same file within a short time window.
pub struct DebouncedEventProcessor {
    /// Receiver for raw processed events.
    event_receiver: Receiver<ProcessedEvent>,
    /// Sender for debounced events.
    debounced_sender: Sender<DebouncedEvent>,
    /// Configuration for debouncing behavior.
    config: FileWatcherConfig,
    /// Set of pending events being debounced.
    pending_events: Arc<RwLock<HashSet<PathBuf>>>,
}

impl DebouncedEventProcessor {
    /// Creates a new debounced event processor.
    ///
    /// # Arguments
    ///
    /// * `event_receiver` - Receiver for raw processed events.
    /// * `debounced_sender` - Sender for debounced events.
    /// * `config` - Configuration for debouncing behavior.
    ///
    /// # Returns
    ///
    /// A new `DebouncedEventProcessor`.
    #[must_use]
    pub fn new(
        event_receiver: Receiver<ProcessedEvent>,
        debounced_sender: Sender<DebouncedEvent>,
        config: FileWatcherConfig,
    ) -> Self {
        Self {
            event_receiver,
            debounced_sender,
            config,
            pending_events: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Starts the debounced event processing loop.
    ///
    /// This method should be run in a dedicated task/thread.
    pub async fn start_processing(self) {
        let mut changed_files = Vec::new();
        let mut removed_files = Vec::new();
        let mut renamed_files = Vec::new();

        loop {
            // Wait for the first event
            match self.event_receiver.recv().await {
                Ok(event) => {
                    self.process_event(
                        event,
                        &mut changed_files,
                        &mut removed_files,
                        &mut renamed_files,
                    );
                }
                Err(e) => {
                    error!("Error receiving file system event: {}", e);
                    break;
                }
            }

            // Enter debounce window - collect more events until timeout
            let sleep = sleep(Duration::from_millis(self.config.debounce_delay_ms));
            pin!(sleep);

            loop {
                select! {
                    () = &mut sleep => {
                        // Timeout reached, stop collecting and send batch
                        break;
                    }
                    res = self.event_receiver.recv() => {
                        match res {
                            Ok(event) => {
                                self.process_event(
                                    event,
                                    &mut changed_files,
                                    &mut removed_files,
                                    &mut renamed_files,
                                );
                            }
                            Err(e) => {
                                error!("Channel closed while debouncing: {}", e);

                                // Don't break outer loop yet, flush current batch first
                                break;
                            }
                        }
                    }
                }
            }

            // Send debounced events if we have any
            if !changed_files.is_empty() {
                let _ = self
                    .debounced_sender
                    .send(FilesChanged {
                        paths: take(&mut changed_files),
                    })
                    .await;
            }
            if !removed_files.is_empty() {
                let _ = self
                    .debounced_sender
                    .send(FilesRemoved {
                        paths: take(&mut removed_files),
                    })
                    .await;
            }
            if !renamed_files.is_empty() {
                let _ = self
                    .debounced_sender
                    .send(FilesRenamed {
                        paths: take(&mut renamed_files),
                    })
                    .await;
            }

            // Clear pending events tracking
            self.pending_events.write().clear();
        }
    }

    /// Helper to process a single event and update buffers
    fn process_event(
        &self,
        event: ProcessedEvent,
        changed_files: &mut Vec<PathBuf>,
        removed_files: &mut Vec<PathBuf>,
        renamed_files: &mut Vec<(PathBuf, PathBuf)>,
    ) {
        match event {
            FileChanged { path, .. } => {
                if !self.pending_events.read().contains(&path) {
                    self.pending_events.write().insert(path.clone());
                    changed_files.push(path);
                }
            }
            FileRemoved { path } => {
                if !self.pending_events.read().contains(&path) {
                    self.pending_events.write().insert(path.clone());
                    removed_files.push(path);
                }
            }
            FileRenamed { from, to } => {
                if !self.pending_events.read().contains(&from)
                    && !self.pending_events.read().contains(&to)
                {
                    self.pending_events.write().insert(from.clone());
                    self.pending_events.write().insert(to.clone());
                    renamed_files.push((from, to));
                }
            }
        }
    }
}

//! Debounced event processor that handles rapid file changes.

use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use {
    async_channel::{Receiver, Sender},
    parking_lot::RwLock,
    tokio::time::sleep,
    tracing::error,
};

use crate::library::file_watcher::{
    config::FileWatcherConfig,
    events::{DebouncedEvent, ProcessedEvent},
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
            match self.event_receiver.recv().await {
                Ok(event) => {
                    match event {
                        ProcessedEvent::FileChanged { path, .. } => {
                            if self.pending_events.read().contains(&path) {
                                // Already pending, skip
                                continue;
                            }
                            self.pending_events.write().insert(path.clone());
                            changed_files.push(path);
                        }
                        ProcessedEvent::FileRemoved { path } => {
                            if self.pending_events.read().contains(&path) {
                                // Already pending, skip
                                continue;
                            }
                            self.pending_events.write().insert(path.clone());
                            removed_files.push(path);
                        }
                        ProcessedEvent::FileRenamed { from, to } => {
                            if self.pending_events.read().contains(&from)
                                || self.pending_events.read().contains(&to)
                            {
                                // Already pending, skip
                                continue;
                            }
                            self.pending_events.write().insert(from.clone());
                            self.pending_events.write().insert(to.clone());
                            renamed_files.push((from, to));
                        }
                    }

                    // Wait for debounce delay
                    sleep(Duration::from_millis(self.config.debounce_delay_ms)).await;

                    // Send debounced events if we have any
                    if !changed_files.is_empty() {
                        let _ = self
                            .debounced_sender
                            .send(DebouncedEvent::FilesChanged {
                                paths: std::mem::take(&mut changed_files),
                            })
                            .await;
                    }
                    if !removed_files.is_empty() {
                        let _ = self
                            .debounced_sender
                            .send(DebouncedEvent::FilesRemoved {
                                paths: std::mem::take(&mut removed_files),
                            })
                            .await;
                    }
                    if !renamed_files.is_empty() {
                        let _ = self
                            .debounced_sender
                            .send(DebouncedEvent::FilesRenamed {
                                paths: std::mem::take(&mut renamed_files),
                            })
                            .await;
                    }

                    // Clear pending events
                    self.pending_events.write().clear();
                }
                Err(e) => {
                    error!("Error receiving file system event: {}", e);
                    break;
                }
            }
        }
    }
}

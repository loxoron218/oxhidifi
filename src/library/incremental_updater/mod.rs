//! Incremental update coordinator for efficient library updates.
//!
//! This module implements efficient incremental database updates that avoid
//! full library rescans while maintaining data consistency and referential integrity.

mod config;
mod event_processing;

use std::sync::Arc;

use {
    async_channel::Receiver,
    tokio::{spawn, task::JoinHandle},
    tracing::{debug, error, warn},
};

use crate::{
    error::domain::LibraryError,
    library::{
        database::LibraryDatabase, dr_parser::DrParser, file_watcher::events::DebouncedEvent,
        incremental_updater::config::IncrementalUpdaterConfig,
    },
};

/// Coordinates incremental library updates.
///
/// The `IncrementalUpdater` processes file system events in batches,
/// maintains referential integrity, and provides atomic database updates.
pub struct IncrementalUpdater {
    /// Database interface.
    database: Arc<LibraryDatabase>,
    /// DR parser (optional).
    dr_parser: Option<Arc<DrParser>>,
    /// Configuration.
    config: IncrementalUpdaterConfig,
}

impl IncrementalUpdater {
    /// Creates a new incremental updater.
    ///
    /// # Arguments
    ///
    /// * `database` - Database interface.
    /// * `settings` - User settings.
    /// * `config` - Optional configuration.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `IncrementalUpdater` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if initialization fails.
    pub fn new(
        database: Arc<LibraryDatabase>,
        config: Option<IncrementalUpdaterConfig>,
    ) -> Result<Self, LibraryError> {
        let config = config.unwrap_or_default();

        // Create DR parser if enabled
        let dr_parser = if config.enable_dr_parsing {
            match DrParser::new(Arc::clone(&database)) {
                Ok(parser) => Some(Arc::new(parser)),
                Err(e) => {
                    warn!(error = %e, "Failed to initialize DR parser");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            database,
            dr_parser,
            config,
        })
    }

    /// Starts processing debounced events from a receiver.
    ///
    /// # Arguments
    ///
    /// * `receiver` - Receiver for debounced events.
    ///
    /// # Returns
    ///
    /// A task handle for the processing loop.
    #[must_use]
    pub fn start_processing(&self, receiver: Receiver<DebouncedEvent>) -> JoinHandle<()> {
        let database = Arc::clone(&self.database);
        let dr_parser = self.dr_parser.clone();
        let config = self.config.clone();

        spawn(async move {
            Self::process_events_loop(receiver, database, dr_parser, config).await;
        })
    }

    /// Main event processing loop.
    async fn process_events_loop(
        receiver: Receiver<DebouncedEvent>,
        database: Arc<LibraryDatabase>,
        dr_parser: Option<Arc<DrParser>>,
        config: IncrementalUpdaterConfig,
    ) {
        while let Ok(event) = receiver.recv().await {
            match event {
                DebouncedEvent::FilesChanged { paths } => {
                    debug!("Processing {} changed files incrementally", paths.len());
                    if let Err(e) = event_processing::handle_files_changed_incremental(
                        paths,
                        &database,
                        dr_parser.as_ref(),
                        &config,
                    )
                    .await
                    {
                        error!(error = %e, "Error handling changed files incrementally");
                    }
                }
                DebouncedEvent::FilesRemoved { paths } => {
                    debug!("Processing {} removed files incrementally", paths.len());
                    if let Err(e) =
                        event_processing::handle_files_removed_incremental(paths, &database).await
                    {
                        error!(error = %e, "Error handling removed files incrementally");
                    }
                }
                DebouncedEvent::FilesRenamed { paths } => {
                    debug!("Processing {} renamed files incrementally", paths.len());
                    if let Err(e) = event_processing::handle_files_renamed_incremental(
                        paths,
                        &database,
                        dr_parser.as_ref(),
                        &config,
                    )
                    .await
                    {
                        error!(error = %e, "Error handling renamed files incrementally");
                    }
                }
            }
        }
    }

    /// Gets the current configuration.
    ///
    /// # Returns
    ///
    /// A reference to the current `IncrementalUpdaterConfig`.
    #[must_use]
    pub fn config(&self) -> &IncrementalUpdaterConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use crate::library::incremental_updater::IncrementalUpdaterConfig;

    #[test]
    fn incremental_updater_config_default() {
        let config = IncrementalUpdaterConfig::default();
        assert_eq!(config.max_batch_size, 50);
        assert!(config.enable_dr_parsing);
    }
}

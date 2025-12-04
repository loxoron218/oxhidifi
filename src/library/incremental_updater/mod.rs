//! Incremental update coordinator for efficient library updates.
//!
//! This module implements efficient incremental database updates that avoid
//! full library rescans while maintaining data consistency and referential integrity.

use std::sync::Arc;

use {
    async_channel::Receiver,
    tokio::task::JoinHandle,
    tracing::{debug, error},
};

use crate::{
    config::settings::UserSettings,
    error::domain::LibraryError,
    library::{database::LibraryDatabase, dr_parser::DrParser, file_watcher::DebouncedEvent},
};

mod config;
mod handlers;

pub use config::IncrementalUpdaterConfig;

/// Coordinates incremental library updates.
///
/// The `IncrementalUpdater` processes file system events in batches,
/// maintains referential integrity, and provides atomic database updates.
pub struct IncrementalUpdater {
    /// Database interface.
    database: Arc<LibraryDatabase>,
    /// DR parser (optional).
    dr_parser: Option<Arc<DrParser>>,
    /// User settings.
    settings: Arc<parking_lot::RwLock<UserSettings>>,
    /// Configuration.
    config: IncrementalUpdaterConfig,
    /// Task handles for background operations.
    _tasks: Vec<JoinHandle<()>>,
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
    pub async fn new(
        database: Arc<LibraryDatabase>,
        settings: Arc<parking_lot::RwLock<UserSettings>>,
        config: Option<IncrementalUpdaterConfig>,
    ) -> Result<Self, LibraryError> {
        let config = config.unwrap_or_default();

        // Create DR parser if enabled
        let dr_parser = if config.enable_dr_parsing {
            Some(Arc::new(DrParser::new(database.clone())))
        } else {
            None
        };

        let tasks = Vec::new();

        Ok(IncrementalUpdater {
            database,
            dr_parser,
            settings,
            config,
            _tasks: tasks,
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
    pub fn start_processing(&self, receiver: Receiver<DebouncedEvent>) -> JoinHandle<()> {
        let database = self.database.clone();
        let dr_parser = self.dr_parser.clone();
        let settings = self.settings.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            Self::process_events_loop(receiver, database, dr_parser, settings, config).await;
        })
    }

    /// Main event processing loop.
    async fn process_events_loop(
        receiver: Receiver<DebouncedEvent>,
        database: Arc<LibraryDatabase>,
        dr_parser: Option<Arc<DrParser>>,
        settings: Arc<parking_lot::RwLock<UserSettings>>,
        config: IncrementalUpdaterConfig,
    ) {
        while let Ok(event) = receiver.recv().await {
            match event {
                DebouncedEvent::FilesChanged { paths } => {
                    debug!("Processing {} changed files incrementally", paths.len());
                    if let Err(e) = handlers::handle_files_changed_incremental(
                        paths, &database, &dr_parser, &settings, &config,
                    )
                    .await
                    {
                        error!("Error handling changed files incrementally: {}", e);
                    }
                }
                DebouncedEvent::FilesRemoved { paths } => {
                    debug!("Processing {} removed files incrementally", paths.len());
                    if let Err(e) =
                        handlers::handle_files_removed_incremental(paths, &database).await
                    {
                        error!("Error handling removed files incrementally: {}", e);
                    }
                }
                DebouncedEvent::FilesRenamed { paths } => {
                    debug!("Processing {} renamed files incrementally", paths.len());
                    if let Err(e) = handlers::handle_files_renamed_incremental(
                        paths, &database, &dr_parser, &settings, &config,
                    )
                    .await
                    {
                        error!("Error handling renamed files incrementally: {}", e);
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
    pub fn config(&self) -> &IncrementalUpdaterConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use crate::library::incremental_updater::IncrementalUpdaterConfig;

    #[test]
    fn test_incremental_updater_config_default() {
        let config = IncrementalUpdaterConfig::default();
        assert_eq!(config.max_batch_size, 50);
        assert_eq!(config.enable_dr_parsing, true);
    }
}

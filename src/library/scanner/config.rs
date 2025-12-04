//! Configuration for library scanning behavior.

use crate::library::file_watcher::FileWatcherConfig;

/// Configuration for library scanning behavior.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// File watcher configuration.
    pub file_watcher_config: FileWatcherConfig,
    /// Maximum number of concurrent metadata extraction tasks.
    pub max_concurrent_metadata_tasks: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            file_watcher_config: FileWatcherConfig::default(),
            max_concurrent_metadata_tasks: 4,
        }
    }
}

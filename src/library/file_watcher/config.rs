//! Configuration for file watcher behavior.

/// Configuration for file watcher behavior.
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    /// Debounce delay to handle rapid file changes.
    pub debounce_delay_ms: u64,
    /// Whether to monitor hidden files and directories.
    pub include_hidden: bool,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            debounce_delay_ms: 500,
            include_hidden: false,
        }
    }
}
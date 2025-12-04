//! Configuration for incremental updates.

/// Configuration for incremental updates.
#[derive(Debug, Clone)]
pub struct IncrementalUpdaterConfig {
    /// Maximum number of files to process in a single batch.
    pub max_batch_size: usize,
    /// Whether to enable DR value parsing during updates.
    pub enable_dr_parsing: bool,
}

impl Default for IncrementalUpdaterConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 50,
            enable_dr_parsing: true,
        }
    }
}

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed},
    },
    time::Duration,
};

/// Performance metrics collector for monitoring application performance.
///
/// This struct provides atomic counters for various performance metrics
/// including scan times, database operations, cache hits/misses, and
/// image processing times. All operations are thread-safe and use atomic
/// operations for maximum performance.
///
/// # Example
///
/// ```
/// use crate::utils::performance_monitor::get_metrics;
///
/// // Record a database operation
/// get_metrics().record_db_operation();
///
/// // Record a cache hit
/// get_metrics().record_cache_hit();
/// ```
pub struct PerformanceMetrics {
    /// Total time spent scanning files in milliseconds
    total_scan_time: AtomicU64,
    /// Total number of database operations performed
    total_db_operations: AtomicUsize,
    /// Number of cache hits (successful cache lookups)
    cache_hits: AtomicUsize,
    /// Number of cache misses (unsuccessful cache lookups)
    cache_misses: AtomicUsize,
    /// Total time spent processing images in milliseconds
    total_image_processing_time: AtomicU64,
    /// Total number of files processed during scanning
    total_files_processed: AtomicUsize,
}

impl PerformanceMetrics {
    /// Creates a new performance metrics collector with all counters initialized to zero.
    ///
    /// # Returns
    ///
    /// A new PerformanceMetrics instance with all metrics set to zero
    pub fn new() -> Self {
        Self {
            total_scan_time: AtomicU64::new(0),
            total_db_operations: AtomicUsize::new(0),
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
            total_image_processing_time: AtomicU64::new(0),
            total_files_processed: AtomicUsize::new(0),
        }
    }

    /// Records time spent scanning files.
    ///
    /// This function adds the provided duration to the total scan time counter.
    ///
    /// # Arguments
    ///
    /// * `duration` - The duration to add to the total scan time
    pub fn record_scan_time(&self, duration: Duration) {
        self.total_scan_time
            .fetch_add(duration.as_millis() as u64, Relaxed);
    }

    /// Records a database operation.
    ///
    /// This function increments the total database operations counter by one.
    pub fn record_db_operation(&self) {
        self.total_db_operations.fetch_add(1, Relaxed);
    }

    /// Records a cache hit.
    ///
    /// This function increments the cache hits counter by one.
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Relaxed);
    }

    /// Records a cache miss.
    ///
    /// This function increments the cache misses counter by one.
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Relaxed);
    }

    /// Records time spent processing images.
    ///
    /// This function adds the provided duration to the total image processing time counter.
    ///
    /// # Arguments
    ///
    /// * `duration` - The duration to add to the total image processing time
    pub fn record_image_processing_time(&self, duration: Duration) {
        self.total_image_processing_time
            .fetch_add(duration.as_millis() as u64, Relaxed);
    }

    /// Records a processed file.
    ///
    /// This function increments the total files processed counter by one.
    pub fn record_file_processed(&self) {
        self.total_files_processed.fetch_add(1, Relaxed);
    }

    /// Gets a snapshot of the current metrics.
    ///
    /// This function returns a PerformanceStats struct containing the current
    /// values of all performance metrics. The returned snapshot is consistent
    /// but may not reflect real-time changes that occur after the snapshot is taken.
    ///
    /// # Returns
    ///
    /// A PerformanceStats struct containing the current metric values
    pub fn get_stats(&self) -> PerformanceStats {
        PerformanceStats {
            total_scan_time_ms: self.total_scan_time.load(Relaxed),
            total_db_operations: self.total_db_operations.load(Relaxed),
            cache_hits: self.cache_hits.load(Relaxed),
            cache_misses: self.cache_misses.load(Relaxed),
            total_image_processing_time_ms: self.total_image_processing_time.load(Relaxed),
            total_files_processed: self.total_files_processed.load(Relaxed),
        }
    }

    /// Resets all metrics to zero.
    ///
    /// This function sets all performance counters back to zero, effectively
    /// clearing all accumulated metrics data.
    pub fn reset(&self) {
        self.total_scan_time.store(0, Relaxed);
        self.total_db_operations.store(0, Relaxed);
        self.cache_hits.store(0, Relaxed);
        self.cache_misses.store(0, Relaxed);
        self.total_image_processing_time.store(0, Relaxed);
        self.total_files_processed.store(0, Relaxed);
    }
}

/// Snapshot of performance metrics at a specific point in time.
///
/// This struct contains the current values of all performance metrics
/// captured when `PerformanceMetrics::get_stats()` is called. It provides
/// calculated metrics like cache hit ratios and average processing times.
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// Total time spent scanning files in milliseconds
    pub total_scan_time_ms: u64,
    /// Total number of database operations performed
    pub total_db_operations: usize,
    /// Number of cache hits (successful cache lookups)
    pub cache_hits: usize,
    /// Number of cache misses (unsuccessful cache lookups)
    pub cache_misses: usize,
    /// Total time spent processing images in milliseconds
    pub total_image_processing_time_ms: u64,
    /// Total number of files processed during scanning
    pub total_files_processed: usize,
}

impl PerformanceStats {
    /// Gets the cache hit ratio as a value between 0.0 and 1.0.
    ///
    /// The cache hit ratio represents the effectiveness of the cache,
    /// with 1.0 indicating all cache lookups were successful.
    ///
    /// # Returns
    ///
    /// A f64 value between 0.0 and 1.0 representing the cache hit ratio,
    /// or 0.0 if no cache operations have occurred
    pub fn cache_hit_ratio(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Gets the average time spent processing each file in milliseconds.
    ///
    /// This metric helps understand the efficiency of file processing operations.
    ///
    /// # Returns
    ///
    /// A f64 value representing the average time per file in milliseconds,
    /// or 0.0 if no files have been processed
    pub fn avg_time_per_file_ms(&self) -> f64 {
        if self.total_files_processed == 0 {
            0.0
        } else {
            self.total_scan_time_ms as f64 / self.total_files_processed as f64
        }
    }
}

/// Global performance metrics collector instance.
///
/// This static provides thread-safe access to the application's performance
/// metrics. It is initialized on first access and remains available for
/// the lifetime of the application.
pub static METRICS: OnceLock<PerformanceMetrics> = OnceLock::new();

/// Gets the global performance metrics instance, initializing it if needed.
///
/// This function provides access to the singleton PerformanceMetrics instance.
/// The instance is created on first access using PerformanceMetrics::new().
///
/// # Returns
///
/// A reference to the global PerformanceMetrics instance
pub fn get_metrics() -> &'static PerformanceMetrics {
    METRICS.get_or_init(PerformanceMetrics::new)
}

/// Formats the performance metrics as a human-readable string.
///
/// This function retrieves the current performance metrics and formats them
/// in a user-friendly way for display in the UI or logs.
///
/// # Returns
///
/// A formatted string containing all performance metrics
pub fn format_metrics() -> String {
    let stats = get_metrics().get_stats();
    format!(
        "Performance Metrics:
- Total scan time: {} ms
- Files processed: {}
- Average time per file: {:.2} ms
- Database operations: {}
- Cache hits: {}
- Cache misses: {}
- Cache hit ratio: {:.2}%
- Image processing time: {} ms",
        stats.total_scan_time_ms,
        stats.total_files_processed,
        stats.avg_time_per_file_ms(),
        stats.total_db_operations,
        stats.cache_hits,
        stats.cache_misses,
        stats.cache_hit_ratio() * 100.0,
        stats.total_image_processing_time_ms
    )
}

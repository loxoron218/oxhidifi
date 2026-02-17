//! Buffer configuration for audio ring buffers.
//!
//! This module provides configurable buffer sizes to allow
//! memory-conscious configurations for embedded or low-memory environments.

use serde::{Deserialize, Serialize};

/// Buffer size configuration for audio ring buffers.
///
/// These sizes must be powers of 2 for efficient rtrb ring buffer bitmask wrapping.
/// Larger buffers provide smoother playback but consume more memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    /// Main ring buffer size (producer → consumer) in samples.
    pub main_buffer_size: usize,
    /// Resampler ring buffer size in samples.
    pub resampler_buffer_size: usize,
    /// Input buffer size for reading samples in the resampling loop.
    pub input_buffer_size: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            main_buffer_size: 65536,
            resampler_buffer_size: 65536,
            input_buffer_size: 32768,
        }
    }
}

impl BufferConfig {
    /// Creates a low-memory configuration for embedded or low-end systems.
    ///
    /// Uses smaller buffers (16K samples) to reduce memory footprint.
    ///
    /// # Returns
    ///
    /// A new `BufferConfig` with reduced buffer sizes.
    #[must_use]
    pub fn low_memory() -> Self {
        Self {
            main_buffer_size: 16384,
            resampler_buffer_size: 16384,
            input_buffer_size: 8192,
        }
    }

    /// Validates that all buffer sizes are powers of 2.
    ///
    /// # Returns
    ///
    /// `true` if all sizes are powers of 2, `false` otherwise.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        is_power_of_two(self.main_buffer_size)
            && is_power_of_two(self.resampler_buffer_size)
            && is_power_of_two(self.input_buffer_size)
    }
}

/// Checks if a number is a power of 2.
///
/// # Returns
///
/// `true` if `n` is a power of 2, `false` otherwise.
fn is_power_of_two(n: usize) -> bool {
    n > 0 && n.is_power_of_two()
}

#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use crate::audio::buffer_config::BufferConfig;

    #[test]
    fn test_default_config() {
        let config = BufferConfig::default();
        assert_eq!(config.main_buffer_size, 65536);
        assert_eq!(config.resampler_buffer_size, 65536);
        assert_eq!(config.input_buffer_size, 32768);
    }

    #[test]
    fn test_low_memory_config() {
        let config = BufferConfig::low_memory();
        assert_eq!(config.main_buffer_size, 16384);
        assert_eq!(config.resampler_buffer_size, 16384);
        assert_eq!(config.input_buffer_size, 8192);
    }

    #[test]
    fn test_is_valid_default() {
        let config = BufferConfig::default();
        assert!(config.is_valid());
    }

    #[test]
    fn test_is_valid_low_memory() {
        let config = BufferConfig::low_memory();
        assert!(config.is_valid());
    }

    #[test]
    fn test_is_valid_invalid_sizes() {
        let config = BufferConfig {
            main_buffer_size: 1000,
            resampler_buffer_size: 65536,
            input_buffer_size: 32768,
        };
        assert!(!config.is_valid());
    }

    #[test]
    fn test_buffer_config_serialization() {
        let config = BufferConfig::default();
        let serialized = to_string(&config).unwrap();
        let deserialized: BufferConfig = from_str(&serialized).unwrap();
        assert_eq!(config.main_buffer_size, deserialized.main_buffer_size);
    }
}

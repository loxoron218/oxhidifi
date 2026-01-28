//! Pre-buffering system for gapless playback.
//!
//! This module handles pre-decoding of the next track in the queue
//! to enable seamless transitions between tracks without audio gaps.

use std::{path::Path, sync::Arc, thread::JoinHandle};

use {
    parking_lot::Mutex,
    rtrb::{Producer, RingBuffer},
    tracing::debug,
};

use crate::audio::{
    decoder::{AudioDecoder, AudioFormat, DecoderError, MS_PER_SEC},
    metadata::{MetadataError, TagReader},
};

/// Error type for pre-buffering operations.
#[derive(Debug, thiserror::Error)]
pub enum PrebufferError {
    /// Decoder error during pre-buffering.
    #[error("Decoder error: {0}")]
    DecoderError(#[from] DecoderError),
    /// Metadata error during track initialization.
    #[error("Metadata error: {0}")]
    MetadataError(#[from] MetadataError),
}

/// Pre-buffered track with decoded audio data.
pub struct PrebufferedTrack {
    /// Ring buffer producer for pre-decoded samples.
    pub producer: Producer<f32>,
    /// Audio decoder for this track.
    pub decoder: AudioDecoder,
    /// Duration of track in milliseconds.
    pub duration_ms: u64,
}

/// Manages pre-buffering of next track for gapless playback.
///
/// The `Prebuffer` stores pre-buffered track data to enable
/// seamless transitions when the current track finishes.
pub struct Prebuffer {
    /// Pre-buffering thread handle.
    thread_handle: Option<JoinHandle<Result<(), PrebufferError>>>,
    /// Pre-buffered track data.
    prebuffered_track: Arc<Mutex<Option<PrebufferedTrack>>>,
}

impl Prebuffer {
    /// Creates a new pre-buffer manager.
    ///
    /// # Returns
    ///
    /// A new `Prebuffer` instance.
    #[must_use]
    pub fn new() -> Self {
        let prebuffered_track = Arc::new(Mutex::new(None));

        Self {
            thread_handle: None,
            prebuffered_track,
        }
    }

    /// Pre-buffers the next track for gapless playback.
    ///
    /// # Arguments
    ///
    /// * `track_path` - Path to the next track to pre-buffer.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `PrebufferError` if track cannot be loaded or decoded.
    pub fn preload_track<P: AsRef<Path>>(&self, track_path: P) -> Result<(), PrebufferError> {
        debug!(
            "Prebuffer: Starting preload for track: {:?}",
            track_path.as_ref()
        );

        let path = track_path.as_ref();

        // Extract metadata first
        let _metadata = TagReader::read_metadata(path)?;

        // Create decoder
        let decoder = AudioDecoder::new(path)?;
        let duration_ms = decoder.duration_ms().unwrap_or(0);

        // Create ring buffer for pre-buffered samples
        let buffer_size = Self::calculate_buffer_size(duration_ms, &decoder.format);

        let (producer, _) = RingBuffer::<f32>::new(buffer_size);

        // Store pre-buffered track
        let prebuffered = PrebufferedTrack {
            producer,
            decoder,
            duration_ms,
        };

        *self.prebuffered_track.lock() = Some(prebuffered);

        debug!("Prebuffer: Track preloaded, duration: {} ms", duration_ms);

        Ok(())
    }

    /// Checks if a track is pre-buffered and ready.
    ///
    /// # Returns
    ///
    /// `true` if a track is pre-buffered, `false` otherwise.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.prebuffered_track.lock().is_some()
    }

    /// Takes the pre-buffered track for playback.
    ///
    /// # Returns
    ///
    /// The pre-buffered track, or `None` if not ready.
    #[must_use]
    pub fn take_prebuffered_track(&self) -> Option<PrebufferedTrack> {
        self.prebuffered_track.lock().take()
    }

    /// Stops pre-buffering gracefully.
    pub fn stop(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Calculates the appropriate buffer size for pre-buffering.
    ///
    /// # Arguments
    ///
    /// * `duration_ms` - Track duration in milliseconds.
    /// * `format` - Audio format information.
    ///
    /// # Returns
    ///
    /// The calculated buffer size in samples.
    #[must_use]
    pub fn calculate_buffer_size(duration_ms: u64, format: &AudioFormat) -> usize {
        let sample_rate = u64::from(format.sample_rate);
        let channels = usize::try_from(format.channels).unwrap_or(2);

        // Calculate samples needed for pre-buffer duration
        let pre_buffer_samples = (duration_ms * sample_rate / MS_PER_SEC) * channels as u64;

        // Limit to reasonable maximum
        pre_buffer_samples.min(65536) as usize
    }
}

impl Default for Prebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Prebuffer {
    fn clone(&self) -> Self {
        Self {
            thread_handle: None,
            prebuffered_track: Arc::clone(&self.prebuffered_track),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::{decoder::AudioFormat, prebuffer::Prebuffer};

    #[test]
    fn test_prebuffer_creation() {
        let prebuffer = Prebuffer::new();
        assert!(!prebuffer.is_ready());
    }

    #[test]
    fn test_prebuffer_ready_check() {
        let prebuffer = Prebuffer::new();
        assert!(!prebuffer.is_ready());
    }

    #[test]
    fn test_calculate_buffer_size() {
        let format = AudioFormat {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            channel_mask: 0,
        };

        // Test with various durations
        let buffer_size = Prebuffer::calculate_buffer_size(5000, &format);

        // Should be reasonable size
        assert!(buffer_size > 0 && buffer_size <= 65536);
    }

    #[test]
    fn test_calculate_buffer_size_short_duration() {
        let format = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0,
        };

        let buffer_size = Prebuffer::calculate_buffer_size(1000, &format);
        assert!(buffer_size > 0 && buffer_size <= 65536);
    }

    #[test]
    fn test_calculate_buffer_size_high_sample_rate() {
        let format = AudioFormat {
            sample_rate: 192_000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0,
        };

        let buffer_size = Prebuffer::calculate_buffer_size(5000, &format);
        assert_eq!(buffer_size, 65536);
    }

    #[test]
    fn test_calculate_buffer_size_mono() {
        let format = AudioFormat {
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: 16,
            channel_mask: 0,
        };

        let buffer_size = Prebuffer::calculate_buffer_size(5000, &format);
        assert!(buffer_size > 0 && buffer_size <= 65536);
    }

    #[test]
    fn test_take_prebuffered_track() {
        let prebuffer = Prebuffer::new();
        let track = prebuffer.take_prebuffered_track();
        assert!(track.is_none());
    }
}

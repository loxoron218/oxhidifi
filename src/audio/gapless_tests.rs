//! Tests for gapless playback functionality.

#[cfg(test)]
mod tests {
    use crate::audio::{decoder::AudioFormat, prebuffer::Prebuffer};

    #[test]
    fn test_prebuffer_preload_nonexistent_file() {
        let prebuffer = Prebuffer::new();

        let result = prebuffer.preload_track("/nonexistent/file.flac");

        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Metadata error") || err_msg.contains("Failed to read audio file")
        );
    }

    #[test]
    fn test_prebuffer_take_empty() {
        let prebuffer = Prebuffer::new();
        let track = prebuffer.take_prebuffered_track();
        assert!(track.is_none());
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size() {
        let format = AudioFormat {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            channel_mask: 0,
        };

        let buffer_size = Prebuffer::calculate_buffer_size(5000, &format);
        assert!(buffer_size <= 65536);
        assert!(buffer_size > 0);
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size_short() {
        let format = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0,
        };

        let buffer_size = Prebuffer::calculate_buffer_size(1000, &format);
        assert!(buffer_size <= 65536);
        assert!(buffer_size > 0);
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size_high_sample_rate() {
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
    fn test_prebuffer_calculate_buffer_size_mono() {
        let format = AudioFormat {
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: 16,
            channel_mask: 0,
        };

        let buffer_size = Prebuffer::calculate_buffer_size(5000, &format);
        assert!(buffer_size <= 65536);
        assert!(buffer_size > 0);
    }

    #[test]
    fn test_prebuffer_stop() {
        let mut prebuffer = Prebuffer::new();
        prebuffer.stop();
        assert!(!prebuffer.is_ready());
    }

    #[test]
    fn test_prebuffer_default() {
        let prebuffer = Prebuffer::default();
        assert!(!prebuffer.is_ready());
    }

    #[test]
    fn test_prebuffer_clone() {
        let prebuffer = Prebuffer::new();
        let _cloned = prebuffer.clone();
    }
}

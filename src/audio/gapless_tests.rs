//! Tests for gapless playback functionality.

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use crate::audio::{
        buffer_config::BufferConfig, decoder_types::AudioFormat, prebuffer::Prebuffer,
    };

    #[test]
    fn test_prebuffer_preload_nonexistent_file() -> Result<()> {
        let prebuffer = Prebuffer::new();

        let result = prebuffer.preload_track("/nonexistent/file.flac");

        let Err(err) = result else {
            bail!("Expected error, got Ok");
        };
        let err_msg = err.to_string();
        if !err_msg.contains("Metadata error") && !err_msg.contains("Failed to read audio file") {
            bail!(
                "Expected error containing 'Metadata error' or 'Failed to read audio file', got '{err_msg}'"
            );
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_take_empty() -> Result<()> {
        let prebuffer = Prebuffer::new();
        let track = prebuffer.take_prebuffered_track();
        if track.is_some() {
            bail!("Expected None, got Some");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size() -> Result<()> {
        let prebuffer = Prebuffer::new();
        let format = AudioFormat {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            channel_mask: 0,
        };

        let max_size = BufferConfig::default().main_buffer_size;
        let buffer_size = prebuffer.calculate_buffer_size(5000, &format);
        if buffer_size > max_size {
            bail!("Expected buffer size <= {max_size}, got {buffer_size}");
        }
        if buffer_size == 0 {
            bail!("Buffer size should be positive");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size_short() -> Result<()> {
        let prebuffer = Prebuffer::new();
        let format = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0,
        };

        let max_size = BufferConfig::default().main_buffer_size;
        let buffer_size = prebuffer.calculate_buffer_size(5000, &format);
        if buffer_size > max_size {
            bail!("Expected buffer size <= {max_size}, got {buffer_size}");
        }
        if buffer_size == 0 {
            bail!("Expected positive buffer size, got 0");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size_high_sample_rate() -> Result<()> {
        let prebuffer = Prebuffer::new();
        let format = AudioFormat {
            sample_rate: 192_000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0,
        };

        let max_size = BufferConfig::default().main_buffer_size;
        let buffer_size = prebuffer.calculate_buffer_size(5000, &format);
        if buffer_size != max_size {
            bail!("Expected {max_size}, got {buffer_size}");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_calculate_buffer_size_mono() -> Result<()> {
        let prebuffer = Prebuffer::new();
        let format = AudioFormat {
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: 16,
            channel_mask: 0,
        };

        let max_size = BufferConfig::default().main_buffer_size;
        let buffer_size = prebuffer.calculate_buffer_size(5000, &format);
        if buffer_size > max_size {
            bail!("Expected buffer size <= {max_size}, got {buffer_size}");
        }
        if buffer_size == 0 {
            bail!("Expected positive buffer size, got 0");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_stop() -> Result<()> {
        let mut prebuffer = Prebuffer::new();
        prebuffer.stop();
        if prebuffer.is_ready() {
            bail!("Expected false, got true");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_default() -> Result<()> {
        let prebuffer = Prebuffer::default();
        if prebuffer.is_ready() {
            bail!("Expected false, got true");
        }
        Ok(())
    }

    #[test]
    fn test_prebuffer_clone() {
        let prebuffer = Prebuffer::new();
        let _cloned = prebuffer;
    }
}

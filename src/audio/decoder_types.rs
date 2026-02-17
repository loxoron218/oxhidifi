//! Common types used by audio decoder.

use std::io::Error as StdError;

use {
    serde::{Deserialize, Serialize},
    symphonia::core::errors::Error as SymphoniaError,
    thiserror::Error,
};

/// Error type for audio decoding operations.
#[derive(Error, Debug)]
pub enum DecoderError {
    /// Failed to open or read the audio file.
    #[error("IO error: {0}")]
    IoError(#[from] StdError),
    /// Symphonia decoding error.
    #[error("Decoding error: {0}")]
    SymphoniaError(#[from] SymphoniaError),
    /// Unsupported audio format.
    #[error("Unsupported audio format")]
    UnsupportedFormat,
    /// No audio track found in file.
    #[error("No audio track found")]
    NoAudioTrack,
    /// Failed to create audio buffer.
    #[error("Failed to create audio buffer")]
    BufferCreationFailed,
    /// Invalid track index (cannot fit in u32).
    #[error("Invalid track index")]
    InvalidTrackIndex,
}

/// Audio format information extracted during decoding setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFormat {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u32,
    /// Bits per sample.
    pub bits_per_sample: u32,
    /// Channel mask (for surround sound).
    pub channel_mask: u16,
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind::NotFound};

    use crate::audio::decoder_types::{
        AudioFormat,
        DecoderError::{IoError, UnsupportedFormat},
    };

    #[test]
    fn test_decoder_error_display() {
        let io_error = Error::new(NotFound, "File not found");
        let decoder_error = IoError(io_error);
        assert!(decoder_error.to_string().contains("IO error"));

        let unsupported_error = UnsupportedFormat;
        assert_eq!(unsupported_error.to_string(), "Unsupported audio format");
    }

    #[test]
    fn test_audio_format_creation() {
        let format = AudioFormat {
            sample_rate: 96000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0x3,
        };
        assert_eq!(format.sample_rate, 96000);
        assert_eq!(format.channels, 2);
    }
}

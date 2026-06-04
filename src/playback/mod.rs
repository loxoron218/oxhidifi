//! Audio playback pipeline: decoder, resampler, output, queue, gapless transitions.

pub mod decoder;
pub mod engine;
pub mod gapless;
pub mod output;
pub mod queue;
pub mod resampler;

use std::{
    io::{Result, Write},
    path::PathBuf,
};

use thiserror::Error;

/// Errors originating from the decoder subsystem.
#[derive(Debug, Error)]
pub enum DecoderError {
    /// Failed to open the audio file.
    #[error("Failed to open file: {0}")]
    OpenError(String),
    /// Failed to decode audio frames.
    #[error("Decode error: {0}")]
    DecodeError(String),
    /// Unsupported codec or format.
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    /// Track not found at path.
    #[error("Track not found: {0}")]
    TrackNotFound(PathBuf),
    /// End of stream reached.
    #[error("End of stream")]
    EndOfStream,
}

/// Errors originating from the audio output subsystem.
#[derive(Debug, Error)]
pub enum OutputError {
    /// No audio device available.
    #[error("No audio device available")]
    NoDeviceAvailable,
    /// Device disconnected during playback.
    #[error("Device disconnected: {0}")]
    DeviceDisconnected(String),
    /// Failed to configure the audio stream.
    #[error("Stream configuration error: {0}")]
    StreamConfigError(String),
    /// General output error.
    #[error("Output error: {0}")]
    Output(String),
}

/// Errors originating from the playback engine.
#[derive(Debug, Error)]
pub enum PlaybackError {
    /// Track not found in library.
    #[error("Track not found: {0}")]
    TrackNotFound(i64),
    /// Error from decoder.
    #[error("Decoder error: {0}")]
    DecoderError(#[from] DecoderError),
    /// Error from audio output.
    #[error("Output device error: {0}")]
    Output(#[from] OutputError),
    /// Audio device disconnected.
    #[error("Device disconnected")]
    DeviceDisconnected,
    /// No audio device is available.
    #[error("No device available")]
    NoDeviceAvailable,
    /// Playback queue is empty.
    #[error("Queue empty")]
    QueueEmpty,
}

/// Write a WAV file header (PCM, mono/stereo). Does not write audio data.
///
/// After calling this, write `data_size` bytes of sample data to `writer`.
///
/// # Errors
///
/// Returns `std::io::Error` if writing to `writer` fails.
pub fn write_wav_header<W: Write>(
    writer: &mut W,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    data_size: u32,
) -> Result<()> {
    let riff_size = 36u32 + data_size;

    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_size.to_le_bytes())?;
    writer.write_all(b"WAVE")?;
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(
        &(sample_rate * u32::from(channels) * u32::from(bits_per_sample / 8)).to_le_bytes(),
    )?;
    writer.write_all(&(channels * (bits_per_sample / 8)).to_le_bytes())?;
    writer.write_all(&bits_per_sample.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&data_size.to_le_bytes())?;

    Ok(())
}

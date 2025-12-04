//! Audio file decoding using the `symphonia` crate.
//!
//! This module handles audio file format detection, decoding, and provides
//! decoded audio samples to the output system via ring buffers.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use rtrb::{Consumer, Producer};
use symphonia::{
    core::{
        audio::{AudioBufferRef, SignalSpec},
        codecs::{DecoderOptions, CODEC_TYPE_NULL},
        errors::Error as SymphoniaError,
        formats::{FormatOptions, FormatReader},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
        units::Time,
    },
    default::get_probe,
};
use thiserror::Error;

use crate::audio::metadata::{TagReader, TechnicalMetadata};

/// Error type for audio decoding operations.
#[derive(Error, Debug)]
pub enum DecoderError {
    /// Failed to open or read the audio file.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
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
}

/// Audio format information extracted during decoding setup.
#[derive(Debug, Clone)]
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

/// Audio decoder that reads and decodes audio files.
///
/// The decoder is responsible for opening audio files, detecting their format,
/// and providing decoded audio samples to the output system.
pub struct AudioDecoder {
    /// The underlying format reader.
    format_reader: Box<dyn FormatReader>,
    /// The active audio track index.
    track_index: usize,
    /// Audio format information.
    pub format: AudioFormat,
    /// Technical metadata from the file.
    pub technical_metadata: TechnicalMetadata,
}

impl AudioDecoder {
    /// Creates a new audio decoder for the specified file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the audio file to decode.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `AudioDecoder` or a `DecoderError`.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError` if:
    /// - The file cannot be opened or read
    /// - The file format is unsupported
    /// - No audio track is found in the file
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, DecoderError> {
        let path = path.as_ref();
        
        // Extract technical metadata first
        let technical_metadata = TagReader::read_metadata(path)
            .map_err(|e| DecoderError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))?
            .technical;

        // Create media source stream
        let file = std::fs::File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Create format hint
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }

        // Probe the format
        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();
        let probe = get_probe();

        let probed = probe.format(&hint, mss, &format_opts, &metadata_opts)
            .map_err(DecoderError::SymphoniaError)?;

        let mut format_reader = probed.format;

        // Find the first audio track
        let track_index = format_reader
            .tracks()
            .iter()
            .position(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(DecoderError::NoAudioTrack)?;

        // Select the audio track
        format_reader
            .seek(Time::from_ms(0), None)
            .map_err(DecoderError::SymphoniaError)?;

        let track = &format_reader.tracks()[track_index];
        let codec_params = &track.codec_params;

        let format = AudioFormat {
            sample_rate: codec_params.sample_rate.unwrap_or(44100),
            channels: codec_params.channels.unwrap_or(symphonia::core::audio::Channels::FRONT_LEFT | 
                                                    symphonia::core::audio::Channels::FRONT_RIGHT)
                .count() as u32,
            bits_per_sample: codec_params.bits_per_coded_sample.unwrap_or(16),
            channel_mask: codec_params.channel_mask.unwrap_or(0),
        };

        Ok(AudioDecoder {
            format_reader,
            track_index,
            format,
            technical_metadata,
        })
    }

    /// Decodes the next packet of audio data.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option<AudioBufferRef>` or a `DecoderError`.
    /// Returns `None` when the end of the file is reached.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError` if decoding fails.
    pub fn decode_next_packet(&mut self) -> Result<Option<AudioBufferRef>, DecoderError> {
        loop {
            match self.format_reader.next_packet() {
                Ok(packet) => {
                    // Skip non-audio packets
                    if packet.track_id() != self.track_index as u32 {
                        continue;
                    }

                    // Decode the packet
                    let decoder = self.format_reader.codec(self.track_index)
                        .map_err(DecoderError::SymphoniaError)?;
                    
                    let decoded = decoder.decode(&packet)
                        .map_err(DecoderError::SymphoniaError)?;
                    
                    return Ok(Some(decoded));
                }
                Err(symphonia::core::errors::Error::IoError(e)) => {
                    return Err(DecoderError::IoError(e));
                }
                Err(symphonia::core::errors::Error::ResetRequired) => {
                    // Try to reset the decoder
                    let _ = self.format_reader.codec_mut(self.track_index)
                        .map_err(DecoderError::SymphoniaError)?
                        .reset();
                    continue;
                }
                Err(symphonia::core::errors::Error::DecodeError(_)) => {
                    // Skip corrupted packets and continue
                    continue;
                }
                Err(symphonia::core::errors::Error::SeekError(_)) => {
                    // End of file reached
                    return Ok(None);
                }
                Err(e) => {
                    return Err(DecoderError::SymphoniaError(e));
                }
            }
        }
    }

    /// Seeks to the specified time position in milliseconds.
    ///
    /// # Arguments
    ///
    /// * `position_ms` - Target position in milliseconds.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError` if seeking fails.
    pub fn seek(&mut self, position_ms: u64) -> Result<(), DecoderError> {
        let time = Time::from_ms(position_ms as i64);
        self.format_reader.seek(time, None)
            .map_err(DecoderError::SymphoniaError)?;
        Ok(())
    }

    /// Gets the duration of the audio file in milliseconds.
    ///
    /// # Returns
    ///
    /// Duration in milliseconds, or `None` if unknown.
    pub fn duration_ms(&self) -> Option<u64> {
        self.format_reader
            .tracks()
            .get(self.track_index)
            .and_then(|track| track.codec_params.n_frames)
            .map(|frames| {
                let sample_rate = self.format.sample_rate as f64;
                (frames as f64 / sample_rate * 1000.0) as u64
            })
    }
}

/// Audio producer that feeds decoded samples into a ring buffer.
///
/// This struct wraps an `AudioDecoder` and continuously decodes audio,
/// writing the samples to the provided ring buffer producer.
pub struct AudioProducer {
    decoder: AudioDecoder,
    producer: Producer<f32>,
}

impl AudioProducer {
    /// Creates a new audio producer.
    ///
    /// # Arguments
    ///
    /// * `decoder` - The audio decoder to use.
    /// * `producer` - The ring buffer producer to write samples to.
    pub fn new(decoder: AudioDecoder, producer: Producer<f32>) -> Self {
        Self { decoder, producer }
    }

    /// Runs the audio production loop.
    ///
    /// This method continuously decodes audio and writes samples to the ring buffer.
    /// It should be run on a dedicated worker thread.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError` if decoding fails.
    pub fn run(mut self) -> Result<(), DecoderError> {
        loop {
            match self.decoder.decode_next_packet()? {
                Some(buffer) => {
                    // Convert audio buffer to f32 samples
                    let samples = match buffer {
                        AudioBufferRef::F32(buf) => buf.chan(0).to_vec(),
                        AudioBufferRef::I16(buf) => {
                            buf.chan(0).iter().map(|&sample| sample as f32 / 32768.0).collect()
                        }
                        AudioBufferRef::U16(buf) => {
                            buf.chan(0).iter().map(|&sample| (sample as i16 - 32768) as f32 / 32768.0).collect()
                        }
                        AudioBufferRef::I24(buf) => {
                            buf.chan(0).iter().map(|&sample| sample as f32 / 8388608.0).collect()
                        }
                        AudioBufferRef::I32(buf) => {
                            buf.chan(0).iter().map(|&sample| sample as f32 / 2147483648.0).collect()
                        }
                        _ => return Err(DecoderError::UnsupportedFormat),
                    };

                    // Write samples to ring buffer
                    let mut written = 0;
                    while written < samples.len() {
                        match self.producer.push_slice(&samples[written..]) {
                            Ok(count) if count > 0 => written += count,
                            Ok(_) => {
                                // Buffer is full, wait a bit and retry
                                std::thread::sleep(std::time::Duration::from_micros(100));
                            }
                            Err(_) => {
                                // Ring buffer disconnected, exit gracefully
                                return Ok(());
                            }
                        }
                    }
                }
                None => {
                    // End of file reached
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let decoder_error = DecoderError::IoError(io_error);
        assert!(decoder_error.to_string().contains("IO error"));

        let unsupported_error = DecoderError::UnsupportedFormat;
        assert_eq!(unsupported_error.to_string(), "Unsupported audio format");
    }

    #[test]
    fn test_audio_format_creation() {
        let format = AudioFormat {
            sample_rate: 96000,
            channels: 2,
            bits_per_sample: 24,
            channel_mask: 0x3, // Stereo
        };
        assert_eq!(format.sample_rate, 96000);
        assert_eq!(format.channels, 2);
    }
}
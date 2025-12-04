//! Audio file decoding using the `symphonia` crate.
//!
//! This module handles audio file format detection, decoding, and provides
//! decoded audio samples to the output system via ring buffers.

use std::{
    fs::File,
    io::{Error as StdError, ErrorKind::InvalidData},
    path::Path,
    thread::sleep,
    time::Duration,
};

use {
    rtrb::{Producer, PushError::Full},
    serde::{Deserialize, Serialize},
    symphonia::{
        core::{
            audio::{
                AudioBufferRef::{self, F32, U16, U24, U32},
                Signal,
            },
            codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions},
            errors::Error as SymphoniaError,
            formats::{FormatOptions, FormatReader, SeekMode::Accurate, SeekTo::Time},
            io::MediaSourceStream,
            meta::MetadataOptions,
            probe::Hint,
            units::Time as OtherTime,
        },
        default::{get_codecs, get_probe},
    },
    thiserror::Error,
};

use crate::audio::metadata::{TagReader, TechnicalMetadata};

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

/// Audio decoder that reads and decodes audio files.
///
/// The decoder is responsible for opening audio files, detecting their format,
/// and providing decoded audio samples to the output system.
pub struct AudioDecoder {
    /// The underlying format reader.
    format_reader: Box<dyn FormatReader>,
    /// The active audio decoder.
    decoder: Option<Box<dyn Decoder>>,
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
            .map_err(|e| DecoderError::IoError(StdError::new(InvalidData, e.to_string())))?
            .technical;

        // Create media source stream
        let file = File::open(path)?;
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

        let probed = probe
            .format(&hint, mss, &format_opts, &metadata_opts)
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
            .seek(
                Accurate,
                Time {
                    time: OtherTime::new(0, 0.0),
                    track_id: None,
                },
            )
            .map_err(DecoderError::SymphoniaError)?;

        let track = &format_reader.tracks()[track_index];
        let codec_params = &track.codec_params;

        let format = AudioFormat {
            sample_rate: codec_params.sample_rate.unwrap_or(44100),
            channels: codec_params.channels.map(|ch| ch.count()).unwrap_or(2) as u32,
            bits_per_sample: codec_params.bits_per_coded_sample.unwrap_or(16),
            channel_mask: 0,
        };

        // Create decoder
        let decoder = get_codecs()
            .make(codec_params, &DecoderOptions::default())
            .map_err(DecoderError::SymphoniaError)?;

        Ok(AudioDecoder {
            format_reader,
            decoder: Some(decoder),
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
    pub fn decode_next_packet(&mut self) -> Result<Option<AudioBufferRef<'_>>, DecoderError> {
        loop {
            match self.format_reader.next_packet() {
                Ok(packet) => {
                    // Skip non-audio packets
                    if packet.track_id() != self.track_index as u32 {
                        continue;
                    }

                    // Decode the packet
                    if let Some(ref mut decoder) = self.decoder {
                        let decoded = decoder
                            .decode(&packet)
                            .map_err(DecoderError::SymphoniaError)?;

                        return Ok(Some(decoded));
                    } else {
                        return Err(DecoderError::SymphoniaError(SymphoniaError::Unsupported(
                            "No decoder available",
                        )));
                    }
                }
                Err(SymphoniaError::IoError(e)) => {
                    return Err(DecoderError::IoError(e));
                }
                Err(SymphoniaError::ResetRequired) => {
                    // Try to reset the decoder - not directly supported, skip for now
                    continue;
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    // Skip corrupted packets and continue
                    continue;
                }
                Err(SymphoniaError::SeekError(_)) => {
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
        let time = OtherTime::new(position_ms / 1000, ((position_ms % 1000) as f64) / 1000.0);
        self.format_reader
            .seek(
                Accurate,
                Time {
                    time,
                    track_id: None,
                },
            )
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
                        F32(buf) => buf.chan(0).to_vec(),
                        U16(buf) => buf
                            .chan(0)
                            .iter()
                            .map(|&sample| sample as f32 / 65535.0)
                            .collect(),
                        U24(buf) => {
                            // Handle u24 properly by converting to f32
                            // u24 samples are stored as 32-bit integers with the upper 8 bits unused
                            buf.chan(0)
                                .iter()
                                .map(|&sample| {
                                    // Extract the lower 24 bits and normalize to [-1.0, 1.0]
                                    let sample_u32 = sample.0;
                                    let sample_24 = sample_u32 & 0x00FFFFFF;
                                    if sample_24 & 0x00800000 != 0 {
                                        // Negative number (sign bit set)
                                        let signed_sample = sample_24 as i32 - 0x01000000;
                                        signed_sample as f32 / 8388608.0
                                    } else {
                                        // Positive number
                                        sample_24 as f32 / 8388607.0
                                    }
                                })
                                .collect()
                        }
                        U32(buf) => buf
                            .chan(0)
                            .iter()
                            .map(|&sample| sample as f32 / 4294967295.0)
                            .collect(),
                        _ => return Err(DecoderError::UnsupportedFormat),
                    };

                    // Write samples to ring buffer
                    for &sample in &samples {
                        loop {
                            match self.producer.push(sample) {
                                Ok(()) => break,
                                Err(Full(_)) => {
                                    // Buffer is full, wait a bit and retry
                                    sleep(Duration::from_micros(100));
                                }
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
    use std::io::{Error, ErrorKind::NotFound};

    use crate::audio::decoder::{AudioFormat, DecoderError};

    #[test]
    fn test_decoder_error_display() {
        let io_error = Error::new(NotFound, "File not found");
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

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
    num_traits::cast::ToPrimitive,
    rtrb::{Producer, PushError::Full},
    serde::{Deserialize, Serialize},
    symphonia::{
        core::{
            audio::{
                AudioBufferRef::{self, F32, F64, S8, S16, S24, S32, U8, U16, U24, U32},
                Signal, SignalSpec,
            },
            codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions},
            errors::Error as SymphoniaError,
            formats::{FormatOptions, FormatReader, SeekMode::Accurate, SeekTo::Time},
            io::{MediaSourceStream, MediaSourceStreamOptions},
            meta::MetadataOptions,
            probe::Hint,
            units::Time as OtherTime,
        },
        default::{get_codecs, get_probe},
    },
    thiserror::Error,
};

use crate::audio::metadata::{TagReader, TechnicalMetadata};

/// Sleep duration when producer buffer is full.
const PRODUCER_SLEEP_DURATION: Duration = Duration::from_micros(100);

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
    /// Signal specification from symphonia (sample rate + channel layout).
    pub signal_spec: SignalSpec,
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
        let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        // Create format hint
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }

        // Probe the format
        let format_opts: FormatOptions = FormatOptions::default();
        let metadata_opts: MetadataOptions = MetadataOptions::default();
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

        let signal_spec = SignalSpec::new(
            codec_params.sample_rate.unwrap_or(44100),
            codec_params.channels.ok_or(DecoderError::NoAudioTrack)?,
        );

        let format = AudioFormat {
            sample_rate: signal_spec.rate,
            channels: u32::try_from(signal_spec.channels.count()).unwrap_or(2),
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
            signal_spec,
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
    ///
    /// # Panics
    ///
    /// Panics if `self.track_index` cannot fit in a `u32`. This should never happen in practice
    /// as track indices are always small integers.
    pub fn decode_next_packet(&mut self) -> Result<Option<AudioBufferRef<'_>>, DecoderError> {
        loop {
            match self.format_reader.next_packet() {
                Ok(packet) => {
                    // Skip non-audio packets
                    if packet.track_id() != u32::try_from(self.track_index).unwrap() {
                        continue;
                    }

                    // Decode the packet
                    if let Some(ref mut decoder) = self.decoder {
                        let decoded = decoder
                            .decode(&packet)
                            .map_err(DecoderError::SymphoniaError)?;

                        return Ok(Some(decoded));
                    }
                    return Err(DecoderError::SymphoniaError(SymphoniaError::Unsupported(
                        "No decoder available",
                    )));
                }
                Err(SymphoniaError::IoError(e)) => {
                    return Err(DecoderError::IoError(e));
                }
                Err(SymphoniaError::ResetRequired | SymphoniaError::DecodeError(_)) => {
                    // Skip corrupted packets and continue
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
        let seconds = position_ms / 1000;
        let frac_ms = (position_ms % 1000) as u32;
        let time = OtherTime::new(seconds, f64::from(frac_ms) / 1000.0);
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
    #[must_use]
    pub fn duration_ms(&self) -> Option<u64> {
        self.format_reader
            .tracks()
            .get(self.track_index)
            .and_then(|track| track.codec_params.n_frames)
            .map(|frames| {
                let sample_rate = u64::from(self.format.sample_rate);
                (frames * 1000 + sample_rate / 2) / sample_rate
            })
    }
}

/// Audio producer that feeds decoded samples into a ring buffer.
///
/// This struct wraps an `AudioDecoder` and continuously decodes audio,
/// writing the samples to the provided ring buffer producer.
pub struct AudioProducer {
    /// The audio decoder that provides raw audio samples.
    decoder: AudioDecoder,
    /// Ring buffer producer for writing decoded samples.
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
    ///
    /// # Panics
    ///
    /// Panics if any `to_f32().unwrap()` call fails. This should never happen
    /// as all values are clamped to valid ranges before conversion.
    pub fn run(mut self) -> Result<(), DecoderError> {
        while let Some(buffer) = self.decoder.decode_next_packet()? {
            let spec = buffer.spec();
            let channels = spec.channels.count();

            // Convert audio buffer to f32 samples in INTERLEAVED format
            let samples = match buffer {
                F32(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            samples.push(buf.chan(ch)[frame]);
                        }
                    }
                    samples
                }
                F64(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let sample = buf.chan(ch)[frame].clamp(-1.0_f64, 1.0_f64);
                            samples.push(sample.to_f32().unwrap());
                        }
                    }
                    samples
                }
                U8(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let s = buf.chan(ch)[frame];
                            let v = (f32::from(s) - 128.0_f32) / 127.0_f32;
                            samples.push(v);
                        }
                    }
                    samples
                }
                S8(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            samples.push(f32::from(buf.chan(ch)[frame]) / 127.0);
                        }
                    }
                    samples
                }
                U16(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let s = buf.chan(ch)[frame];
                            let v = (f32::from(s) - 32768.0_f32) / 32767.0_f32;
                            samples.push(v);
                        }
                    }
                    samples
                }
                S16(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let s = buf.chan(ch)[frame];
                            let v = if s == i16::MIN {
                                -1.0
                            } else {
                                f32::from(s) / f32::from(i16::MAX)
                            };
                            samples.push(v);
                        }
                    }
                    samples
                }
                U24(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let sample_u32 = buf.chan(ch)[frame].0 & 0x00FF_FFFF;
                            let sample = f64::from(sample_u32) / 16_777_215.0_f64;
                            samples.push(sample.to_f32().unwrap());
                        }
                    }
                    samples
                }
                S24(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let s = buf.chan(ch)[frame].0 << 8 >> 8;
                            let v = if s == -8_388_608 {
                                -1.0_f32
                            } else {
                                let sample =
                                    (f64::from(s) / 8_388_607.0_f64).clamp(-1.0_f64, 1.0_f64);
                                sample.to_f32().unwrap()
                            };
                            samples.push(v);
                        }
                    }
                    samples
                }
                U32(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let s = buf.chan(ch)[frame];
                            let sample = (f64::from(s) - 2_147_483_648.0_f64) / 2_147_483_647.0_f64;
                            let v = sample.to_f32().unwrap();
                            samples.push(v);
                        }
                    }
                    samples
                }
                S32(buf) => {
                    let mut samples = Vec::with_capacity(buf.frames() * channels);
                    for frame in 0..buf.frames() {
                        for ch in 0..channels {
                            let s = buf.chan(ch)[frame];
                            let v = if s == i32::MIN {
                                -1.0_f32
                            } else {
                                (f64::from(s) / f64::from(i32::MAX))
                                    .clamp(-1.0_f64, 1.0_f64)
                                    .to_f32()
                                    .unwrap()
                            };
                            samples.push(v);
                        }
                    }
                    samples
                }
            };

            // Write samples to ring buffer
            for &sample in &samples {
                loop {
                    if self.producer.is_abandoned() {
                        return Ok(());
                    }
                    match self.producer.push(sample) {
                        Ok(()) => break,
                        Err(Full(_)) => {
                            // Buffer is full, wait a bit and retry
                            sleep(PRODUCER_SLEEP_DURATION);
                        }
                    }
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

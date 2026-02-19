//! Audio file decoding using the `symphonia` crate.
//!
//! This module handles audio file format detection, decoding, and provides
//! decoded audio samples to the output system via ring buffers.

use std::{
    fs::File,
    io::{
        Error,
        ErrorKind::{InvalidData, UnexpectedEof},
    },
    path::Path,
};

use symphonia::{
    core::{
        audio::{AudioBufferRef, SignalSpec},
        codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions},
        errors::Error::{DecodeError, IoError, ResetRequired, SeekError, Unsupported},
        formats::{FormatOptions, FormatReader, SeekMode::Accurate, SeekTo::Time},
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
        probe::Hint,
        units::Time as OtherTime,
    },
    default::{get_codecs, get_probe},
};

use crate::audio::{
    decoder_types::{AudioFormat, DecoderError},
    metadata::{TagReader, TechnicalMetadata},
};

/// Milliseconds per second.
pub const MS_PER_SEC: u64 = 1000;

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
            .map_err(|e| DecoderError::IoError(Error::new(InvalidData, e.to_string())))?
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
            bits_per_sample: technical_metadata.bits_per_sample,
            channel_mask: 0,
        };

        // Create decoder
        let decoder = get_codecs()
            .make(codec_params, &DecoderOptions::default())
            .map_err(DecoderError::SymphoniaError)?;

        Ok(Self {
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
                    if packet.track_id()
                        != u32::try_from(self.track_index)
                            .map_err(|_| DecoderError::InvalidTrackIndex)?
                    {
                        continue;
                    }

                    // Decode the packet
                    if let Some(ref mut decoder) = self.decoder {
                        let decoded = decoder
                            .decode(&packet)
                            .map_err(DecoderError::SymphoniaError)?;

                        return Ok(Some(decoded));
                    }
                    return Err(DecoderError::SymphoniaError(Unsupported(
                        "No decoder available",
                    )));
                }
                Err(IoError(e)) => {
                    if e.kind() == UnexpectedEof {
                        return Ok(None);
                    }
                    return Err(DecoderError::IoError(e));
                }
                Err(ResetRequired | DecodeError(_)) => {
                    // Skip corrupted packets and continue
                }
                Err(SeekError(_)) => {
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
        let seconds = position_ms / MS_PER_SEC;
        let frac_ms = (position_ms % MS_PER_SEC) as u32;
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
                (frames * MS_PER_SEC + sample_rate / 2) / sample_rate
            })
    }
}

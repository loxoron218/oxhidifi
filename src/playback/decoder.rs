//! Symphonia decoder bridge.

use std::{fs::File, path::Path};

use symphonia::{
    core::{
        audio::GenericAudioBufferRef,
        codecs::{
            CodecParameters,
            audio::{AudioDecoder, AudioDecoderOptions},
        },
        errors::Error::{DecodeError, IoError, ResetRequired},
        formats::{
            FormatOptions, FormatReader, SeekMode::Accurate, SeekTo::Time as SeekTime,
            TrackType::Audio as TypeAudio, probe::Hint,
        },
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
        units::{Time, Timestamp},
    },
    default::{get_codecs, get_probe},
};

use crate::playback::DecoderError::{
    self, DecodeError as PlaybackDecodeError, EndOfStream, OpenError, SeekError, UnsupportedFormat,
};

/// Audio parameters extracted from the decoded stream.
#[derive(Debug, Clone, Copy)]
pub struct AudioParams {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: u16,
    /// Total duration of the track in seconds (0.0 if unknown).
    pub duration_seconds: f64,
    /// Bit depth (None for lossy / unknown).
    pub bit_depth: Option<u16>,
}

/// Decoded PCM samples with associated audio parameters.
///
/// The `samples` slice borrows the decoder's pre-allocated buffer so that
/// successive [`Decoder::decode_next`] calls reuse the same allocation instead
/// of allocating a fresh `Vec` per batch (zero allocation on the audio hot
/// path, per Constitution Principle IV).
#[derive(Debug, Clone)]
pub struct DecodedSamples<'a> {
    /// Interleaved f32 PCM samples.
    pub samples: &'a [f32],
    /// Audio parameters for this batch.
    pub params: AudioParams,
}

/// Symphonia decoder wrapper that opens a file and decodes PCM frames.
///
/// Each call to [`Decoder::decode_next`] returns the next batch of interleaved
/// f32 samples. When the stream ends, an empty `samples` vec signals
/// end-of-stream.
pub struct Decoder {
    /// Format reader for the audio container.
    format: Box<dyn FormatReader>,
    /// Audio codec decoder.
    codec: Box<dyn AudioDecoder>,
    /// Audio codec parameters for decoder re-initialization after seek.
    codec_params: CodecParameters,
    /// ID of the active audio track.
    track_id: u32,
    /// Audio parameters of the decoded stream.
    params: AudioParams,
    /// Reusable pre-allocated buffer for decoded samples.
    ///
    /// Reused across `decode_next` calls to avoid per-batch heap allocation.
    buf: Vec<f32>,
}

impl Decoder {
    /// Open an audio file and prepare the decoder.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] if the file cannot be opened, probed, or
    /// decoded.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DecoderError> {
        let path = path.as_ref();
        let src = File::open(path).map_err(|e| OpenError(format!("{}: {e}", path.display())))?;

        let mss = MediaSourceStream::new(Box::new(src), MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let meta_opts = MetadataOptions::default();
        let fmt_opts = FormatOptions::default();

        let format = get_probe()
            .probe(&hint, mss, fmt_opts, meta_opts)
            .map_err(|e| UnsupportedFormat(e.to_string()))?;

        let track = format
            .default_track(TypeAudio)
            .ok_or_else(|| UnsupportedFormat("no audio track found".into()))?;

        let codec_params = track
            .codec_params
            .clone()
            .ok_or_else(|| UnsupportedFormat("track has no audio codec parameters".into()))?;

        let Some(audio_params) = codec_params.audio() else {
            return Err(UnsupportedFormat(
                "track has no audio codec parameters".into(),
            ));
        };

        let track_id = track.id;

        let sample_rate = audio_params.sample_rate.unwrap_or(44100);
        let channels = audio_params
            .channels
            .as_ref()
            .map_or(2, |c| u16::try_from(c.count()).unwrap_or(2));

        let duration_seconds = track
            .time_base
            .zip(track.duration)
            .and_then(|(tb, dur)| {
                let ts = Timestamp::new(i64::try_from(dur.get()).unwrap_or(0));
                tb.calc_time(ts)
            })
            .map_or(0.0, |t| t.as_secs_f64());

        let bit_depth = audio_params
            .bits_per_sample
            .map(|b| u16::try_from(b).unwrap_or(0));

        let params = AudioParams {
            sample_rate,
            channels,
            duration_seconds,
            bit_depth,
        };

        let dec_opts = AudioDecoderOptions::default();
        let codec = get_codecs()
            .make_audio_decoder(audio_params, &dec_opts)
            .map_err(|e| PlaybackDecodeError(e.to_string()))?;

        Ok(Self {
            format,
            codec,
            codec_params,
            track_id,
            params,
            buf: Vec::new(),
        })
    }

    /// Decode the next batch of interleaved f32 PCM samples.
    ///
    /// Returns an empty `samples` slice when the stream has ended.
    ///
    /// The returned slice borrows the decoder's internal pre-allocated buffer;
    /// it is valid only until the next call to [`Decoder::decode_next`] (or
    /// any other mutating method).
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] on decode failure.
    pub fn decode_next(&mut self) -> Result<DecodedSamples<'_>, DecoderError> {
        loop {
            match self.try_decode_one() {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => (),
                Err(EndOfStream) => return Ok(self.empty_samples()),
                Err(e) => return Err(e),
            }
        }
    }

    /// Return an empty sample batch with the current audio params.
    fn empty_samples(&mut self) -> DecodedSamples<'_> {
        self.buf.clear();
        DecodedSamples {
            samples: &self.buf,
            params: self.params,
        }
    }

    /// Attempt to decode a single packet, returning `None` on skip/eos.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError::DecodeError`] if the packet cannot be decoded.
    fn try_decode_one(&mut self) -> Result<Option<DecodedSamples<'_>>, DecoderError> {
        let packet = match self.format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => return Err(EndOfStream),
            Err(ResetRequired) => return Ok(None),
            Err(e) => return Err(PlaybackDecodeError(e.to_string())),
        };

        if packet.track_id != self.track_id {
            return Ok(None);
        }

        let decoded = match self.codec.decode(&packet) {
            Ok(decoded) => decoded,
            Err(IoError(_) | DecodeError(_)) => return Ok(None),
            Err(e) => return Err(PlaybackDecodeError(e.to_string())),
        };

        self.buf.clear();
        copy_interleaved_f32(&decoded, &mut self.buf);
        Ok(Some(DecodedSamples {
            samples: &self.buf,
            params: self.params,
        }))
    }

    /// Returns the audio parameters of the decoded stream.
    #[must_use]
    pub const fn params(&self) -> AudioParams {
        self.params
    }

    /// Returns the current capacity of the pre-allocated sample buffer.
    ///
    /// Only compiled for the `verification-tests` feature, which asserts this
    /// capacity stays constant across `decode_next` calls (zero reallocation
    /// on the audio hot path).
    #[cfg(feature = "verification-tests")]
    #[must_use]
    pub const fn buffer_capacity(&self) -> usize {
        self.buf.capacity()
    }

    /// Seek to a position in seconds.
    ///
    /// Returns the actual position seeked to (may differ slightly from
    /// the requested position due to codec frame boundaries).
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError::SeekError`] if seeking fails.
    pub fn seek_to(&mut self, seconds: f64) -> Result<f64, DecoderError> {
        let time = Time::try_from_secs_f64(seconds)
            .ok_or_else(|| DecoderError::SeekError("invalid seek time".into()))?;

        let seeked_to = self
            .format
            .seek(
                Accurate,
                SeekTime {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| SeekError(format!("seek failed: {e}")))?;

        let Some(audio_params) = self.codec_params.audio() else {
            return Err(SeekError("missing audio codec parameters".into()));
        };
        let dec_opts = AudioDecoderOptions::default();
        self.codec = get_codecs()
            .make_audio_decoder(audio_params, &dec_opts)
            .map_err(|e| SeekError(format!("codec reinit failed: {e}")))?;

        let actual_seconds = self
            .format
            .default_track(TypeAudio)
            .and_then(|t| t.time_base)
            .and_then(|tb| tb.calc_time(seeked_to.actual_ts))
            .map_or(seconds, |t| t.as_secs_f64());

        Ok(actual_seconds)
    }
}

/// Copy decoded audio buffer to interleaved f32 samples.
fn copy_interleaved_f32(buf: &GenericAudioBufferRef<'_>, out: &mut Vec<f32>) {
    buf.copy_to_vec_interleaved(out);
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::{Result, Write},
        path::Path,
    };

    use {
        anyhow::{Result as AnyhowResult, bail},
        tempfile::NamedTempFile,
    };

    use crate::playback::{DecoderError::OpenError, decoder::Decoder, write_wav_header};

    fn write_minimal_wav(path: &Path) -> Result<()> {
        let mut f = File::create(path)?;
        let data_size = 2u32;
        write_wav_header(&mut f, 1, 44100, 16, data_size)?;
        f.write_all(&[0u8, 0u8])?;
        Ok(())
    }

    #[test]
    fn open_nonexistent_file_returns_error() {
        let result = Decoder::open("/nonexistent/path/audio.flac");
        assert!(result.is_err());
        assert!(matches!(result, Err(OpenError(_))));
    }

    #[test]
    fn open_invalid_content_returns_error() -> AnyhowResult<()> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(b"not an audio file")?;
        let result = Decoder::open(tmp.path());
        if result.is_ok() {
            bail!("expected error for invalid audio content");
        }
        Ok(())
    }

    #[test]
    fn decode_next_returns_empty_on_end_of_stream() -> AnyhowResult<()> {
        let tmp = NamedTempFile::new()?;
        write_minimal_wav(tmp.path())?;
        let mut decoder = match Decoder::open(tmp.path()) {
            Ok(d) => d,
            Err(e) => bail!("failed to open test wav: {e}"),
        };
        let batch = decoder.decode_next()?;
        if batch.samples.is_empty() {
            bail!("expected at least one sample batch before EOS");
        }
        let eos = decoder.decode_next()?;
        if !eos.samples.is_empty() {
            bail!("expected empty samples at end of stream");
        }
        Ok(())
    }
}

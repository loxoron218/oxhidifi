//! Symphonia decoder bridge with optional dual-decoder pre-buffering.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

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
}

/// Decoded PCM samples with associated audio parameters.
#[derive(Debug, Clone)]
pub struct DecodedSamples {
    /// Interleaved f32 PCM samples.
    pub samples: Vec<f32>,
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

        let params = AudioParams {
            sample_rate,
            channels,
            duration_seconds,
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
        })
    }

    /// Decode the next batch of interleaved f32 PCM samples.
    ///
    /// Returns an empty `samples` vec when the stream has ended.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] on decode failure.
    pub fn decode_next(&mut self) -> Result<DecodedSamples, DecoderError> {
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
    fn empty_samples(&self) -> DecodedSamples {
        DecodedSamples {
            samples: Vec::new(),
            params: self.params,
        }
    }

    /// Attempt to decode a single packet, returning `None` on skip/eos.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError::DecodeError`] if the packet cannot be decoded.
    fn try_decode_one(&mut self) -> Result<Option<DecodedSamples>, DecoderError> {
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

        let mut samples = Vec::new();
        copy_interleaved_f32(&decoded, &mut samples);
        Ok(Some(DecodedSamples {
            samples,
            params: self.params,
        }))
    }

    /// Returns the audio parameters of the decoded stream.
    #[must_use]
    pub fn params(&self) -> AudioParams {
        self.params
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

/// Dual-decoder state for gapless pre-buffering.
///
/// Manages an active decoder (currently playing) and a pre-loaded decoder
/// (next track, decoded in advance during the last ~1 second of playback).
pub struct DualDecoder {
    /// The currently active decoder.
    active: Option<Decoder>,
    /// The pre-loaded decoder for the next track.
    preloaded: Option<Decoder>,
    /// Path of the next pre-loaded track.
    preloaded_path: Option<PathBuf>,
    /// ID of the next pre-loaded track.
    preloaded_track_id: Option<i64>,
}

impl DualDecoder {
    /// Create a new dual-decoder with no active or pre-loaded decoders.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: None,
            preloaded: None,
            preloaded_path: None,
            preloaded_track_id: None,
        }
    }

    /// Set the active decoder to a newly opened file.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] if the file cannot be opened.
    pub fn start<P: AsRef<Path>>(&mut self, path: P) -> Result<(), DecoderError> {
        let decoder = Decoder::open(path)?;
        self.active = Some(decoder);
        self.preloaded = None;
        self.preloaded_path = None;
        self.preloaded_track_id = None;
        Ok(())
    }

    /// Decode the next batch from the active decoder.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] on decode failure. Returns
    /// [`DecoderError::EndOfStream`] if no active decoder is available.
    pub fn decode_next(&mut self) -> Result<DecodedSamples, DecoderError> {
        self.active
            .as_mut()
            .map_or(Err(EndOfStream), Decoder::decode_next)
    }

    /// Pre-load a decoder for the next track.
    ///
    /// The pre-loaded decoder replaces any previous pre-loaded decoder.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] if the file cannot be opened for decoding.
    pub fn preload<P: AsRef<Path>>(&mut self, path: P, track_id: i64) -> Result<(), DecoderError> {
        let decoder = Decoder::open(path.as_ref())?;
        self.preloaded = Some(decoder);
        self.preloaded_path = Some(path.as_ref().to_path_buf());
        self.preloaded_track_id = Some(track_id);
        Ok(())
    }

    /// Swap the pre-loaded decoder to become active.
    ///
    /// Returns the old active decoder for the caller to consume/drop.
    /// Returns `None` if there is no pre-loaded decoder.
    pub fn swap(&mut self) -> Option<Decoder> {
        let old_active = self.active.take();
        if let Some(preloaded) = self.preloaded.take() {
            self.active = Some(preloaded);
        }
        self.preloaded_path = None;
        self.preloaded_track_id = None;
        old_active
    }

    /// Returns parameters of the active decoder, if any.
    #[must_use]
    pub fn active_params(&self) -> Option<AudioParams> {
        self.active.as_ref().map(Decoder::params)
    }

    /// Returns parameters of the pre-loaded decoder, if any.
    #[must_use]
    pub fn preloaded_params(&self) -> Option<AudioParams> {
        self.preloaded.as_ref().map(Decoder::params)
    }

    /// Returns `true` if a pre-loaded decoder is ready.
    #[must_use]
    pub fn has_preloaded(&self) -> bool {
        self.preloaded.is_some()
    }

    /// Returns the ID of the pre-loaded track, if any.
    #[must_use]
    pub fn preloaded_track_id(&self) -> Option<i64> {
        self.preloaded_track_id
    }

    /// Returns `true` if the active decoder is present.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Stop and clear all decoders.
    pub fn stop(&mut self) {
        self.active = None;
        self.preloaded = None;
        self.preloaded_path = None;
        self.preloaded_track_id = None;
    }
}

impl Default for DualDecoder {
    fn default() -> Self {
        Self::new()
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

    use crate::playback::{
        DecoderError::OpenError,
        decoder::{Decoder, DualDecoder},
        write_wav_header,
    };

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
    fn dual_decoder_starts_empty() {
        let dd = DualDecoder::new();
        assert!(!dd.is_active());
        assert!(!dd.has_preloaded());
    }

    #[test]
    fn dual_decoder_preload_swap() {
        let mut dd = DualDecoder::new();

        let result = dd.start("/nonexistent/file.flac");
        assert!(result.is_err());

        let result = dd.preload("/nonexistent/next.flac", 42);
        assert!(result.is_err());

        assert!(!dd.is_active());
        assert!(!dd.has_preloaded());
    }

    #[test]
    fn dual_decoder_swap_noop() {
        let mut dd = DualDecoder::new();
        let old = dd.swap();
        assert!(old.is_none());
        assert!(!dd.is_active());
    }

    #[test]
    fn dual_decoder_stop_clears_everything() {
        let mut dd = DualDecoder::new();
        dd.stop();
        assert!(!dd.is_active());
        assert!(!dd.has_preloaded());
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

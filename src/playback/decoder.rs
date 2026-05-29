//! Symphonia decoder bridge: open file, decode PCM frames, emit end-of-stream.

use std::{fs::File, path::Path};

use symphonia::{
    core::{
        audio::GenericAudioBufferRef,
        codecs::{
            CodecParameters::Audio,
            audio::{AudioDecoder, AudioDecoderOptions},
        },
        errors::Error::{DecodeError, IoError, ResetRequired},
        formats::{FormatOptions, FormatReader, TrackType::Audio as TypeAudio, probe::Hint},
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
    },
    default::{get_codecs, get_probe},
};

use crate::playback::DecoderError::{
    self, DecodeError as PlaybackDecodeError, OpenError, UnsupportedFormat,
};

/// Audio parameters extracted from the decoded stream.
#[derive(Debug, Clone, Copy)]
pub struct AudioParams {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: u16,
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

        let codec_params = match &track.codec_params {
            Some(Audio(p)) => p.clone(),
            _ => {
                return Err(UnsupportedFormat(
                    "track has no audio codec parameters".into(),
                ));
            }
        };

        let track_id = track.id;

        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params
            .channels
            .as_ref()
            .map_or(2, |c| u16::try_from(c.count()).unwrap_or(2));

        let params = AudioParams {
            sample_rate,
            channels,
        };

        let dec_opts = AudioDecoderOptions::default();
        let codec = get_codecs()
            .make_audio_decoder(&codec_params, &dec_opts)
            .map_err(|e| PlaybackDecodeError(e.to_string()))?;

        Ok(Self {
            format,
            codec,
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
        match self.try_decode_one() {
            Ok(Some(result)) => Ok(result),
            Ok(None) => self.decode_next(),
            Err(e) => Err(e),
        }
    }

    /// Attempt to decode a single packet, returning `None` on skip/eos.
    fn try_decode_one(&mut self) -> Result<Option<DecodedSamples>, DecoderError> {
        let packet = match self.format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(ResetRequired) => return Ok(None),
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
}

/// Copy decoded audio buffer to interleaved f32 samples.
fn copy_interleaved_f32(buf: &GenericAudioBufferRef<'_>, out: &mut Vec<f32>) {
    buf.copy_to_vec_interleaved(out);
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use {
        anyhow::{Result, bail},
        tempfile::NamedTempFile,
    };

    use crate::playback::{DecoderError::OpenError, decoder::Decoder};

    #[test]
    fn open_nonexistent_file_returns_error() {
        let result = Decoder::open("/nonexistent/path/audio.flac");
        assert!(result.is_err());
        assert!(matches!(result, Err(OpenError(_))));
    }

    #[test]
    fn open_invalid_content_returns_error() -> Result<()> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(b"not an audio file")?;
        let result = Decoder::open(tmp.path());
        if result.is_ok() {
            bail!("expected error for invalid audio content");
        }
        Ok(())
    }
}

//! Dual-decoder state machine for gapless pre-buffering.

use std::path::{Path, PathBuf};

use crate::playback::{
    DecoderError::{self, EndOfStream},
    decoder::{AudioParams, DecodedSamples, Decoder},
};

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
    pub const fn new() -> Self {
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
    /// The returned samples borrow the active decoder's pre-allocated buffer.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] on decode failure. Returns
    /// [`DecoderError::EndOfStream`] if no active decoder is available.
    pub fn decode_next(&mut self) -> Result<DecodedSamples<'_>, DecoderError> {
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
    pub fn active_params(&self) -> Option<AudioParams> {
        self.active.as_ref().map(Decoder::params)
    }

    /// Returns parameters of the pre-loaded decoder, if any.
    pub fn preloaded_params(&self) -> Option<AudioParams> {
        self.preloaded.as_ref().map(Decoder::params)
    }

    /// Returns `true` if a pre-loaded decoder is ready.
    #[must_use]
    pub const fn has_preloaded(&self) -> bool {
        self.preloaded.is_some()
    }

    /// Returns the ID of the pre-loaded track, if any.
    #[must_use]
    pub const fn preloaded_track_id(&self) -> Option<i64> {
        self.preloaded_track_id
    }

    /// Returns `true` if the active decoder is present.
    #[must_use]
    pub const fn is_active(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use crate::playback::decoder::dual::DualDecoder;

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
}

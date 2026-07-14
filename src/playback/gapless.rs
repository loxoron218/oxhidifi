//! Gapless track transition logic with pre-buffering and seamless decoder switching.

use std::path::PathBuf;

use rtrb::Consumer;

use crate::playback::{DecoderError, decoder::Decoder};

/// State of the gapless transition engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaplessState {
    /// No playback active.
    Idle,
    /// Playing a track with no next track ready.
    Playing {
        /// ID of the currently playing track.
        track_id: i64,
    },
    /// Playing with the next track pre-buffered.
    PreBuffered {
        /// ID of the currently playing track.
        current_track_id: i64,
        /// ID of the pre-buffered next track.
        next_track_id: i64,
        /// Sample rate of the next track (for reconfiguration).
        next_sample_rate: u32,
    },
}

/// Manages pre-buffering and seamless transitions between tracks.
///
/// Coordinates the dual decoder state, resampler reconfiguration,
/// and event emission during gapless transitions.
/// When disabled, pre-buffering and transitions are skipped entirely.
pub struct GaplessTransitioner {
    /// Current transition state.
    state: GaplessState,
    /// Pre-loaded decoder for the next track, if any.
    preloaded_decoder: Option<Decoder>,
    /// Path to the pre-loaded next track.
    preloaded_path: Option<PathBuf>,
    /// Whether gapless transitions are enabled.
    enabled: bool,
}

impl GaplessTransitioner {
    /// Create a new idle transitioner with gapless enabled by default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: GaplessState::Idle,
            preloaded_decoder: None,
            preloaded_path: None,
            enabled: true,
        }
    }

    /// Enable or disable gapless transitions.
    pub fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.preloaded_decoder = None;
            self.preloaded_path = None;
            self.state = match self.state {
                GaplessState::PreBuffered { .. } => GaplessState::Idle,
                other => other,
            };
        }
        self.enabled = enabled;
    }

    /// Check whether gapless transitions are enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Start playback of a track and reset state.
    pub fn start_playback(&mut self, track_id: i64) {
        self.state = GaplessState::Playing { track_id };
        self.preloaded_decoder = None;
        self.preloaded_path = None;
    }

    /// Pre-buffer the next track by opening a decoder for it.
    ///
    /// Returns `Ok(true)` if pre-buffering succeeded, `Ok(false)` if already
    /// pre-buffered or gapless is disabled, or `Err` if the decoder could not
    /// be opened.
    ///
    /// # Errors
    ///
    /// Returns [`DecoderError`] if the next track cannot be decoded.
    pub fn prebuffer_next(
        &mut self,
        current_track_id: i64,
        next_track_id: i64,
        next_path: PathBuf,
    ) -> Result<bool, DecoderError> {
        if !self.enabled || self.preloaded_decoder.is_some() {
            return Ok(false);
        }

        let decoder = Decoder::open(&next_path)?;
        let params = decoder.params();

        self.preloaded_decoder = Some(decoder);
        self.preloaded_path = Some(next_path);
        self.state = GaplessState::PreBuffered {
            current_track_id,
            next_track_id,
            next_sample_rate: params.sample_rate,
        };

        Ok(true)
    }

    /// Execute a gapless transition from the current track to the
    /// pre-buffered next track.
    ///
    /// Returns the `Decoder` for the next track if a pre-buffered decoder
    /// was available, or `None` if no pre-buffered decoder exists.
    ///
    /// The caller should:
    /// 1. Stop the current decode loop
    /// 2. Drain the output ring buffer
    /// 3. Call `transition()` to get the next decoder
    /// 4. Reconfigure the resampler if the sample rate changed (check `next_sample_rate()`)
    /// 5. Start the new decode loop with the returned decoder
    pub fn transition(&mut self) -> Option<Decoder> {
        let decoder = self.preloaded_decoder.take();
        let GaplessState::PreBuffered {
            next_track_id: next_id,
            ..
        } = self.state
        else {
            return decoder;
        };

        self.state = GaplessState::Playing { track_id: next_id };
        self.preloaded_path = None;

        decoder
    }

    /// Get the sample rate of the next pre-buffered track, if known.
    #[must_use]
    pub fn next_sample_rate(&self) -> Option<u32> {
        match self.state {
            GaplessState::PreBuffered {
                next_sample_rate, ..
            } => Some(next_sample_rate),
            _ => None,
        }
    }

    /// Get the ID of the next pre-buffered track, if any.
    #[must_use]
    pub fn next_track_id(&self) -> Option<i64> {
        match self.state {
            GaplessState::PreBuffered { next_track_id, .. } => Some(next_track_id),
            _ => None,
        }
    }

    /// Get the current transition state.
    #[must_use]
    pub fn state(&self) -> GaplessState {
        self.state
    }

    /// Stop and reset all state. Preserves the `enabled` flag.
    pub fn stop(&mut self) {
        self.state = GaplessState::Idle;
        self.preloaded_decoder = None;
        self.preloaded_path = None;
    }

    /// Returns `true` if a next track has been pre-buffered and is ready
    /// for a gapless transition.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.state, GaplessState::PreBuffered { .. })
    }

    /// Returns `true` if currently playing a track.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        matches!(
            self.state,
            GaplessState::Playing { .. } | GaplessState::PreBuffered { .. }
        )
    }
}

impl Default for GaplessTransitioner {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine whether a resampler reconfiguration is needed based on
/// sample rate change.
///
/// Returns `true` if the two sample rates differ.
#[must_use]
pub fn needs_reconfig(current_rate: u32, next_rate: u32) -> bool {
    current_rate != next_rate
}

/// Drain the output ring buffer by consuming all remaining samples.
///
/// Used during gapless transitions to ensure no stale data remains
/// before the next track's samples enter the buffer.
pub fn drain_buffer(consumer: &mut Consumer<f32>) {
    while consumer.pop().is_ok() {}
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use {
        anyhow::{Result, bail},
        num_traits::NumCast,
        rtrb::{Consumer, Producer, RingBuffer},
    };

    use crate::playback::gapless::{
        GaplessState::{Idle, Playing},
        GaplessTransitioner, drain_buffer, needs_reconfig,
    };

    #[test]
    fn transitioner_starts_idle() {
        let t = GaplessTransitioner::new();
        assert_eq!(t.state(), Idle);
        assert!(!t.is_playing());
        assert!(!t.is_ready());
    }

    #[test]
    fn start_playback_sets_playing_state() {
        let mut t = GaplessTransitioner::new();
        t.start_playback(42);
        assert_eq!(t.state(), Playing { track_id: 42 });
        assert!(t.is_playing());
    }

    #[test]
    fn prebuffer_and_transition() {
        let mut t = GaplessTransitioner::new();
        t.start_playback(1);

        let result = t.prebuffer_next(1, 2, PathBuf::from("/nonexistent/file.flac"));
        assert!(result.is_err(), "expected error for nonexistent file");

        assert_eq!(t.state(), Playing { track_id: 1 });

        let decoder = t.transition();
        assert!(decoder.is_none());
    }

    #[test]
    fn needs_reconfig_detects_changes() {
        assert!(!needs_reconfig(44100, 44100));
        assert!(needs_reconfig(44100, 48000));
        assert!(needs_reconfig(48000, 96000));
        assert!(!needs_reconfig(192_000, 192_000));
    }

    #[test]
    fn stop_resets_state() {
        let mut t = GaplessTransitioner::new();
        t.start_playback(1);
        assert!(t.is_playing());
        t.stop();
        assert_eq!(t.state(), Idle);
        assert!(!t.is_playing());
        assert!(!t.is_ready());
    }

    #[test]
    fn is_playing_reflects_state() {
        let mut t = GaplessTransitioner::new();
        assert!(!t.is_playing());
        t.start_playback(1);
        assert!(t.is_playing());
        t.stop();
        assert!(!t.is_playing());
    }

    #[test]
    fn is_ready_false_without_prebuffer() {
        let mut t = GaplessTransitioner::new();
        t.start_playback(1);
        assert!(!t.is_ready());
    }

    #[test]
    fn drain_buffer_clears_all_samples() {
        let (mut producer, mut consumer) = RingBuffer::new(16);
        for i in 0..10 {
            _ = producer.push(NumCast::from(i).unwrap_or(0.0));
        }
        assert!(!consumer.is_empty());
        drain_buffer(&mut consumer);
        assert!(consumer.is_empty());
    }

    #[test]
    fn drain_buffer_on_empty_is_noop() {
        let (_producer, mut consumer): (Producer<f32>, Consumer<f32>) = RingBuffer::new(16);
        drain_buffer(&mut consumer);
        assert!(consumer.is_empty());
    }

    #[test]
    fn transitioner_handles_sequential_playback() {
        let mut t = GaplessTransitioner::new();
        t.start_playback(1);
        assert!(t.is_playing());
        assert_eq!(t.state(), Playing { track_id: 1 });

        t.start_playback(2);
        assert_eq!(t.state(), Playing { track_id: 2 });
    }

    #[test]
    fn prebuffer_called_twice_returns_false_on_second() -> Result<()> {
        let mut t = GaplessTransitioner::new();
        t.start_playback(1);
        let _result = t.prebuffer_next(1, 2, PathBuf::from("/nonexistent/file.flac"));
        if matches!(
            t.prebuffer_next(1, 3, PathBuf::from("/nonexistent/file2.flac")),
            Ok(true),
        ) {
            bail!("expected second prebuffer to return false");
        }
        Ok(())
    }

    #[test]
    fn next_sample_rate_returns_none_when_not_prebuffered() {
        let t = GaplessTransitioner::new();
        assert!(t.next_sample_rate().is_none());
    }

    #[test]
    fn transition_returns_none_when_not_ready() {
        let mut t = GaplessTransitioner::new();
        assert!(t.transition().is_none());
        t.start_playback(1);
        assert!(t.transition().is_none());
    }

    #[test]
    fn needs_reconfig_edge_cases() {
        assert!(needs_reconfig(8000, 44100));
        assert!(needs_reconfig(192_000, 44100));
        assert!(!needs_reconfig(44100, 44100));
        assert!(needs_reconfig(48000, 44100));
    }
}

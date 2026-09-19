//! Playback control interface: trait definition and implementation on `PlaybackEngine`.
//!
//! The [`PlaybackTransport`] trait lives here alongside the thin delegating
//! `impl` block. Queue-driven playback and engine-control logic live in the
//! sibling `transport/` sub-modules to keep each file under 400 lines.

pub mod engine_controls;
pub mod queue_playback;

use async_channel::{Receiver, unbounded};

use crate::playback::{
    PlaybackError,
    devices::OutputMode,
    engine::PlaybackEngine,
    state::{PlaybackEvent, PlaybackState},
    transport::{
        engine_controls::{
            apply_gapless, apply_muted, apply_output_mode, apply_volume, seek_to_position,
            stop_playback, toggle_pause_or_resume,
        },
        queue_playback::{advance_next, advance_previous, play_list, play_list_at, play_single},
    },
};

impl PlaybackTransport for PlaybackEngine {
    /// Play a specific track by ID.
    fn play_track(&self, track_id: i64) -> Result<(), PlaybackError> {
        play_single(&self.shared, track_id)
    }

    /// Play a list of track IDs starting from `start_index`.
    fn play_at(&self, queue: Vec<i64>, start_index: usize) -> Result<(), PlaybackError> {
        play_list_at(&self.shared, queue, start_index)
    }

    /// Play a list of track IDs as a queue.
    fn play_queue(&self, queue: Vec<i64>) -> Result<(), PlaybackError> {
        play_list(&self.shared, queue)
    }

    /// Toggle between play and pause.
    fn toggle_pause(&self) -> Result<(), PlaybackError> {
        toggle_pause_or_resume(&self.shared)
    }

    /// Stop playback entirely.
    fn stop(&self) -> Result<(), PlaybackError> {
        stop_playback(&self.shared)
    }

    /// Advance to the next track.
    fn next_track(&self) -> Result<(), PlaybackError> {
        advance_next(&self.shared)
    }

    /// Go to the previous track.
    fn previous_track(&self) -> Result<(), PlaybackError> {
        advance_previous(&self.shared)
    }

    /// Set the playback volume.
    fn set_volume(&self, volume: f64) -> Result<(), PlaybackError> {
        apply_volume(&self.shared, volume)
    }

    /// Mute or unmute playback.
    fn set_muted(&self, muted: bool) -> Result<(), PlaybackError> {
        apply_muted(&self.shared, muted)
    }

    /// Set the output mode (resampled vs bit-perfect).
    fn set_output_mode(&self, mode: OutputMode) -> Result<(), PlaybackError> {
        apply_output_mode(&self.shared, mode)
    }

    /// Enable or disable gapless playback.
    fn set_gapless_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        apply_gapless(&self.shared, enabled)
    }

    /// Seek to a position in seconds.
    fn seek_to(&self, position_seconds: f64) -> Result<(), PlaybackError> {
        seek_to_position(&self.shared, position_seconds)
    }

    /// Subscribe to playback events.
    fn subscribe(&self) -> Receiver<PlaybackEvent> {
        let (tx, rx) = unbounded();
        self.shared.event_subs.lock().push(tx);
        rx
    }

    /// Get the current playback state.
    fn state(&self) -> PlaybackState {
        self.shared.state.lock().clone()
    }
}

/// Trait for controlling playback, consumed by the UI layer.
pub trait PlaybackTransport: Send + 'static {
    /// Play a specific track by ID.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if playback cannot start.
    fn play_track(&self, track_id: i64) -> Result<(), PlaybackError>;

    /// Play a list of track IDs as a queue.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if playback cannot start.
    fn play_queue(&self, queue: Vec<i64>) -> Result<(), PlaybackError>;

    /// Play a list of track IDs starting from `start_index`.
    ///
    /// The full queue is set in the given order, but playback begins at
    /// the track at `start_index`, enabling previous-track navigation.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if playback cannot start.
    fn play_at(&self, queue: Vec<i64>, start_index: usize) -> Result<(), PlaybackError>;

    /// Toggle between play and pause.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn toggle_pause(&self) -> Result<(), PlaybackError>;

    /// Stop playback entirely.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn stop(&self) -> Result<(), PlaybackError>;

    /// Advance to the next track.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn next_track(&self) -> Result<(), PlaybackError>;

    /// Go to the previous track.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn previous_track(&self) -> Result<(), PlaybackError>;

    /// Set the playback volume.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn set_volume(&self, volume: f64) -> Result<(), PlaybackError>;

    /// Mute or unmute playback.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn set_muted(&self, muted: bool) -> Result<(), PlaybackError>;

    /// Subscribe to playback events.
    fn subscribe(&self) -> Receiver<PlaybackEvent>;

    /// Get the current playback state.
    fn state(&self) -> PlaybackState;

    /// Set the output mode (resampled vs bit-perfect).
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn set_output_mode(&self, mode: OutputMode) -> Result<(), PlaybackError>;

    /// Enable or disable gapless playback.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] on failure.
    fn set_gapless_enabled(&self, enabled: bool) -> Result<(), PlaybackError>;

    /// Seek to a position in seconds.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if no track is playing.
    fn seek_to(&self, position_seconds: f64) -> Result<(), PlaybackError>;
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, bail};

    use crate::playback::{engine::PlaybackEngine, transport::PlaybackTransport};

    #[test]
    fn transport_delegates_play_track_not_found() {
        let engine = PlaybackEngine::new();
        assert!(engine.play_track(99).is_err());
    }

    #[test]
    fn transport_delegates_volume_clamp() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.set_volume(2.0).map_err(|e| anyhow!("{e}"))?;
        if (engine.state().volume - 1.0).abs() >= f64::EPSILON {
            bail!("volume should clamp to 1.0");
        }
        Ok(())
    }
}

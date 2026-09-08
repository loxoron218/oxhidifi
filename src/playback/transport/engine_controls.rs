//! Engine control helpers for the transport.
//!
//! Extracted from `transport.rs` to keep each file under 400 lines.
//! Implements pause/resume, stop, volume, mute, output-mode, gapless, and
//! seek controls on [`EngineShared`]. The thin [`PlaybackTransport`] impl in
//! the parent module delegates to these helpers.

use std::sync::Arc;

use tracing::info;

use crate::playback::{
    PlaybackError,
    devices::OutputMode::{self, BitPerfect, Resampled},
    engine::{DecodeCommand::Seek, EngineShared},
    gapless::GaplessMode::{Disabled, Enabled},
    pause_resume::toggle_play_pause,
    state::{
        MuteState::{Muted, Unmuted},
        PlaybackEvent::{GaplessEnabledChanged, OutputModeChanged, Seeked, Stopped, VolumeChanged},
        PlaybackStatus::Stopped as StatusStopped,
    },
    transport::queue_playback::play_single,
    worker::stop_decode_task,
};

/// Toggle between play and pause, resuming the saved session when stopped.
///
/// When stopped with a saved track, restarts that track and restores the
/// saved position and duration.
///
/// # Errors
///
/// Returns [`PlaybackError`] if the saved track cannot restart or seek fails.
pub fn toggle_pause_or_resume(shared: &Arc<EngineShared>) -> Result<(), PlaybackError> {
    if shared.state.lock().status != StatusStopped {
        toggle_play_pause(shared);
        return Ok(());
    }

    let (tid, saved_position, saved_duration) = {
        let s = shared.state.lock();
        (s.current_track_id, s.elapsed_seconds, s.duration_seconds)
    };

    let Some(tid) = tid else {
        info!("Toggle pause ignored — not playing");
        return Ok(());
    };

    info!(track_id = tid, "Resuming playback from saved session");
    play_single(shared, tid)?;

    if saved_duration > 0.0 {
        shared.state.lock().duration_seconds = saved_duration;
    }

    if saved_position > 0.0 {
        seek_to_position(shared, saved_position)?;
    }

    Ok(())
}

/// Stop playback entirely and clear the current track state.
///
/// # Errors
///
/// Returns [`PlaybackError`] on failure (currently infallible).
pub fn stop_playback(shared: &Arc<EngineShared>) -> Result<(), PlaybackError> {
    let current_track = shared.state.lock().current_track_id;
    info!(track_id = current_track, "Playback stopped",);
    stop_decode_task(shared);
    let mut state = shared.state.lock();
    state.status = StatusStopped;
    state.current_track_id = None;
    state.current_path = None;
    state.elapsed_seconds = 0.0;
    state.duration_seconds = 0.0;
    drop(state);
    shared.send_event(&Stopped);
    Ok(())
}

/// Set the playback volume, clamped to 0.0–1.0.
///
/// # Errors
///
/// Returns [`PlaybackError`] on failure (currently infallible).
pub fn apply_volume(shared: &Arc<EngineShared>, volume: f64) -> Result<(), PlaybackError> {
    let clamped = volume.clamp(0.0, 1.0);
    info!(volume = clamped, "Volume changed",);
    let guard = shared.output.lock();
    if let Some(output) = guard.as_ref() {
        match output.mode() {
            BitPerfect => output.set_hardware_volume(clamped),
            Resampled => output.set_volume_atomic(clamped),
        }
    }
    drop(guard);
    shared.state.lock().volume = clamped;
    shared.send_event(&VolumeChanged { volume: clamped });
    Ok(())
}

/// Mute or unmute playback, preserving the slider volume.
///
/// # Errors
///
/// Returns [`PlaybackError`] on failure (currently infallible).
pub fn apply_muted(shared: &Arc<EngineShared>, muted: bool) -> Result<(), PlaybackError> {
    let vol = shared.state.lock().volume;
    let new_state = if muted { Muted } else { Unmuted };
    let hw_vol = if muted { 0.0 } else { vol };
    let guard = shared.output.lock();
    if let Some(output) = guard.as_ref() {
        match output.mode() {
            BitPerfect => output.set_hardware_volume(hw_vol),
            Resampled => output.set_volume_atomic(hw_vol),
        }
    }
    drop(guard);
    shared.state.lock().muted = new_state;
    Ok(())
}

/// Set the output mode (resampled vs bit-perfect) and re-apply volume.
///
/// # Errors
///
/// Returns [`PlaybackError`] on failure (currently infallible).
pub fn apply_output_mode(
    shared: &Arc<EngineShared>,
    mode: OutputMode,
) -> Result<(), PlaybackError> {
    info!(
        output_mode = ?mode,
        "Output mode changed",
    );

    if let Some(output) = shared.output.lock().as_mut() {
        output.set_mode(mode);
        let current_vol = shared.state.lock().volume;
        match mode {
            Resampled => output.set_volume_atomic(current_vol),
            BitPerfect => output.set_hardware_volume(current_vol),
        }
    }
    shared.state.lock().output_mode = mode;
    shared.send_event(&OutputModeChanged { mode });
    Ok(())
}

/// Enable or disable gapless playback.
///
/// # Errors
///
/// Returns [`PlaybackError`] on failure (currently infallible).
pub fn apply_gapless(shared: &Arc<EngineShared>, enabled: bool) -> Result<(), PlaybackError> {
    info!(enabled, "Gapless playback toggled",);
    shared.state.lock().gapless_mode = if enabled { Enabled } else { Disabled };
    shared.send_event(&GaplessEnabledChanged { enabled });
    Ok(())
}

/// Seek to a position in seconds, clamped to the track duration.
///
/// # Errors
///
/// Returns [`PlaybackError`] on failure (currently infallible).
pub fn seek_to_position(
    shared: &Arc<EngineShared>,
    position_seconds: f64,
) -> Result<(), PlaybackError> {
    let clamped = {
        let state = shared.state.lock();
        position_seconds.clamp(0.0, state.duration_seconds)
    };
    let cmd_tx = shared.decode_tx.lock();
    if let Some(tx) = cmd_tx.as_ref()
        && tx.try_send(Seek(clamped)).is_err()
    {}
    drop(cmd_tx);
    shared.state.lock().elapsed_seconds = clamped;
    shared.send_event(&Seeked {
        position_seconds: clamped,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::{Result, anyhow, bail};

    use crate::playback::{
        devices::OutputMode::{BitPerfect, Resampled},
        engine::EngineShared,
        state::PlaybackStatus::Stopped,
        transport::engine_controls::{
            apply_gapless, apply_muted, apply_output_mode, apply_volume, seek_to_position,
            stop_playback, toggle_pause_or_resume,
        },
    };

    #[test]
    fn stop_clears_state() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        stop_playback(&shared).map_err(|e| anyhow!("{e}"))?;
        if shared.state.lock().status != Stopped {
            bail!("engine should be stopped after stop_playback");
        }
        Ok(())
    }

    #[test]
    fn toggle_pause_when_stopped_without_track_is_noop() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        toggle_pause_or_resume(&shared).map_err(|e| anyhow!("{e}"))?;
        if shared.state.lock().status != Stopped {
            bail!("engine should remain stopped");
        }
        Ok(())
    }

    #[test]
    fn apply_volume_clamps() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        apply_volume(&shared, 2.0).map_err(|e| anyhow!("{e}"))?;
        if (shared.state.lock().volume - 1.0).abs() >= f64::EPSILON {
            bail!("volume should clamp to 1.0");
        }
        apply_volume(&shared, -0.5).map_err(|e| anyhow!("{e}"))?;
        if shared.state.lock().volume.abs() >= f64::EPSILON {
            bail!("volume should clamp to 0.0");
        }
        Ok(())
    }

    #[test]
    fn apply_muted_preserves_volume() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        apply_volume(&shared, 0.7).map_err(|e| anyhow!("{e}"))?;
        apply_muted(&shared, true).map_err(|e| anyhow!("{e}"))?;
        if (shared.state.lock().volume - 0.7).abs() >= f64::EPSILON {
            bail!("slider volume must be preserved while muted");
        }
        apply_muted(&shared, false).map_err(|e| anyhow!("{e}"))?;
        Ok(())
    }

    #[test]
    fn apply_output_mode_and_gapless() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        apply_output_mode(&shared, BitPerfect).map_err(|e| anyhow!("{e}"))?;
        apply_output_mode(&shared, Resampled).map_err(|e| anyhow!("{e}"))?;
        apply_gapless(&shared, true).map_err(|e| anyhow!("{e}"))?;
        apply_gapless(&shared, false).map_err(|e| anyhow!("{e}"))?;
        Ok(())
    }

    #[test]
    fn seek_clamps_to_duration() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        shared.state.lock().duration_seconds = 200.0;
        seek_to_position(&shared, 500.0).map_err(|e| anyhow!("{e}"))?;
        if (shared.state.lock().elapsed_seconds - 200.0).abs() >= f64::EPSILON {
            bail!("elapsed should clamp to duration");
        }
        Ok(())
    }
}

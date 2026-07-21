//! Playback control interface: trait definition and implementation on `PlaybackEngine`.

use {
    async_channel::{Receiver, unbounded},
    tracing::{error, info, warn},
};

use crate::playback::{
    PlaybackError::{self, QueueEmpty, TrackNotFound},
    engine::{
        DecodeCommand::{Pause, Resume, Seek},
        MuteState::{Muted, Unmuted},
        PlaybackEngine,
        PlaybackEvent::{
            self, GaplessEnabledChanged, OutputModeChanged, Paused, QueueChanged, Resumed, Seeked,
            Stopped, VolumeChanged,
        },
        PlaybackState,
        PlaybackStatus::{Paused as StatusPaused, Playing, Stopped as StatusStopped},
    },
    gapless::GaplessMode::{Disabled, Enabled},
    output::OutputMode::{self, BitPerfect, Resampled},
    worker,
};

/// Trait for controlling playback, consumed by the UI layer.
pub trait PlaybackController: Send + 'static {
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

impl PlaybackController for PlaybackEngine {
    fn play_track(&self, track_id: i64) -> Result<(), PlaybackError> {
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&track_id)
            .cloned()
            .ok_or_else(|| {
                warn!(track_id, "Track not found for playback",);
                TrackNotFound(track_id)
            })?;
        info!(track_id, "Play track command",);
        worker::start_playback(&self.shared, track_id, path);
        Ok(())
    }

    fn play_queue(&self, queue: Vec<i64>) -> Result<(), PlaybackError> {
        if queue.is_empty() {
            warn!(
                queue_len = queue.len(),
                "Play queue command with empty queue"
            );
            return Err(QueueEmpty);
        }
        let queue_len = queue.len();
        info!(queue_len, "Play queue command",);
        self.shared.queue.set_queue(queue.clone());
        self.shared.send_event(&QueueChanged { track_ids: queue });
        let first_id = self.shared.queue.current().ok_or(QueueEmpty)?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&first_id)
            .cloned()
            .ok_or(TrackNotFound(first_id))?;
        worker::start_playback(&self.shared, first_id, path);
        Ok(())
    }

    fn toggle_pause(&self) -> Result<(), PlaybackError> {
        let is_playing = {
            let state = self.shared.state.lock();
            state.status != StatusStopped
        };
        if !is_playing {
            info!("Toggle pause ignored — not playing");
            return Ok(());
        }

        let mut state = self.shared.state.lock();
        let was_paused = state.status == StatusPaused;
        let track_id = state.current_track_id;
        let (event, cmd) = if was_paused {
            state.status = Playing;
            info!(track_id, "Playback resumed");
            (Resumed, Resume)
        } else {
            state.status = StatusPaused;
            info!(track_id, "Playback paused");
            (Paused, Pause)
        };
        drop(state);

        let cmd_tx = self.shared.decode_tx.lock();
        if let Some(tx) = cmd_tx.as_ref()
            && let Err(e) = tx.try_send(cmd)
        {
            error!(error = %e, "Failed to send pause/resume command to decode thread");
        }
        drop(cmd_tx);

        self.shared.send_event(&event);
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        let current_track = self.shared.state.lock().current_track_id;
        info!(track_id = current_track, "Playback stopped",);
        worker::stop_decode_task(&self.shared);
        let mut state = self.shared.state.lock();
        state.status = StatusStopped;
        state.current_track_id = None;
        state.current_path = None;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
        drop(state);
        self.shared.send_event(&Stopped);
        Ok(())
    }

    fn next_track(&self) -> Result<(), PlaybackError> {
        let next_id = self.shared.queue.next().ok_or_else(|| {
            info!("Next track failed — queue empty");
            QueueEmpty
        })?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&next_id)
            .cloned()
            .ok_or(TrackNotFound(next_id))?;
        worker::start_playback(&self.shared, next_id, path);
        Ok(())
    }

    fn previous_track(&self) -> Result<(), PlaybackError> {
        let prev_id = self.shared.queue.previous().ok_or_else(|| {
            info!("Previous track failed — queue empty");
            QueueEmpty
        })?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&prev_id)
            .cloned()
            .ok_or(TrackNotFound(prev_id))?;
        worker::start_playback(&self.shared, prev_id, path);
        Ok(())
    }

    fn set_volume(&self, volume: f64) -> Result<(), PlaybackError> {
        let clamped = volume.clamp(0.0, 1.0);
        info!(volume = clamped, "Volume changed",);
        let guard = self.shared.output.lock();
        if let Some(output) = guard.as_ref() {
            match output.mode() {
                BitPerfect => output.set_hardware_volume(clamped),
                Resampled => output.set_volume_atomic(clamped),
            }
        }
        drop(guard);
        self.shared.state.lock().volume = clamped;
        self.shared.send_event(&VolumeChanged { volume: clamped });
        Ok(())
    }

    fn set_muted(&self, muted: bool) -> Result<(), PlaybackError> {
        let vol = self.shared.state.lock().volume;
        let new_state = if muted { Muted } else { Unmuted };
        let hw_vol = if muted { 0.0 } else { vol };
        let guard = self.shared.output.lock();
        if let Some(output) = guard.as_ref() {
            match output.mode() {
                BitPerfect => output.set_hardware_volume(hw_vol),
                Resampled => output.set_volume_atomic(hw_vol),
            }
        }
        drop(guard);
        self.shared.state.lock().muted = new_state;
        Ok(())
    }

    fn set_output_mode(&self, mode: OutputMode) -> Result<(), PlaybackError> {
        info!(
            output_mode = ?mode,
            "Output mode changed",
        );

        if let Some(output) = self.shared.output.lock().as_mut() {
            output.set_mode(mode);
            let current_vol = self.shared.state.lock().volume;
            match mode {
                Resampled => output.set_volume_atomic(current_vol),
                BitPerfect => output.set_hardware_volume(current_vol),
            }
        }
        self.shared.state.lock().output_mode = mode;
        self.shared.send_event(&OutputModeChanged { mode });
        Ok(())
    }

    fn set_gapless_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        info!(enabled, "Gapless playback toggled",);
        self.shared.state.lock().gapless_mode = if enabled { Enabled } else { Disabled };
        self.shared.send_event(&GaplessEnabledChanged { enabled });
        Ok(())
    }

    fn seek_to(&self, position_seconds: f64) -> Result<(), PlaybackError> {
        let clamped = {
            let state = self.shared.state.lock();
            position_seconds.clamp(0.0, state.duration_seconds)
        };
        let cmd_tx = self.shared.decode_tx.lock();
        if let Some(tx) = cmd_tx.as_ref()
            && tx.try_send(Seek(clamped)).is_err()
        {}
        drop(cmd_tx);
        self.shared.state.lock().elapsed_seconds = clamped;
        self.shared.send_event(&Seeked {
            position_seconds: clamped,
        });
        Ok(())
    }

    fn subscribe(&self) -> Receiver<PlaybackEvent> {
        let (tx, rx) = unbounded();
        self.shared.event_subs.lock().push(tx);
        rx
    }

    fn state(&self) -> PlaybackState {
        self.shared.state.lock().clone()
    }
}

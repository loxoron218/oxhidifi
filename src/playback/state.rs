//! Playback state and event types shared across the engine.

use std::path::PathBuf;

use crate::playback::{
    devices::OutputMode::{self, Resampled},
    gapless::GaplessMode::{self, Enabled},
};

/// Mute state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteState {
    /// Audio is muted.
    Muted,
    /// Audio is unmuted.
    Unmuted,
}

/// Events emitted by the playback engine.
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    /// A track started playing.
    TrackStarted {
        /// Track ID.
        track_id: i64,
    },
    /// Live decoder facts are ready for a track.
    ///
    /// Sent from the decode thread after `Decoder::open` updates the shared
    /// source rate. Lets signal-path subscribers re-publish once the live
    /// rate is fresh instead of showing the previous track's rate.
    TrackFormatReady {
        /// Track ID the decoder facts belong to.
        track_id: i64,
        /// Source sample rate in Hz reported by the decoder.
        sample_rate: u32,
    },
    /// The track finished (end of stream).
    TrackFinished {
        /// ID of the finished track.
        track_id: i64,
    },
    /// The playback queue was replaced or modified.
    QueueChanged {
        /// New set of track IDs in the queue.
        track_ids: Vec<i64>,
    },
    /// Playback was paused.
    Paused,
    /// Playback was resumed.
    Resumed,
    /// Playback was stopped.
    Stopped,
    /// Volume changed.
    VolumeChanged {
        /// New volume level.
        volume: f64,
    },
    /// Output mode changed (resampled / bit-perfect).
    OutputModeChanged {
        /// New output mode.
        mode: OutputMode,
    },
    /// Audio device was lost during playback.
    DeviceLost {
        /// Error description.
        error: String,
    },
    /// An error occurred during playback.
    Error {
        /// Error description.
        error: String,
    },
    /// Gapless playback was enabled or disabled.
    GaplessEnabledChanged {
        /// Whether gapless is now enabled.
        enabled: bool,
    },
    /// Seeked to a new position.
    Seeked {
        /// New position in seconds.
        position_seconds: f64,
    },
    /// Periodic position update (~200ms intervals during playback).
    PositionTick {
        /// Current elapsed playback time in seconds.
        elapsed_seconds: f64,
        /// Total track duration in seconds.
        duration_seconds: f64,
    },
}

/// Current state of the playback engine.
#[derive(Debug, Clone)]
pub struct PlaybackState {
    /// The currently playing track ID, if any.
    pub current_track_id: Option<i64>,
    /// Album ID of the currently playing track (`-1` if none).
    pub current_album_id: i64,
    /// The file path of the currently playing track, if any.
    pub current_path: Option<PathBuf>,
    /// Current playback status.
    pub status: PlaybackStatus,
    /// Current volume (0.0 to 1.0).
    pub volume: f64,
    /// Mute state.
    pub muted: MuteState,
    /// Elapsed playback time in seconds.
    pub elapsed_seconds: f64,
    /// Total track duration in seconds (0.0 if unknown).
    pub duration_seconds: f64,
    /// Gapless playback mode.
    pub gapless_mode: GaplessMode,
    /// Output mode: resampled (software volume) or bit-perfect (hardware volume).
    pub output_mode: OutputMode,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            current_track_id: None,
            current_album_id: -1,
            current_path: None,
            status: PlaybackStatus::Stopped,
            volume: 1.0,
            muted: MuteState::Unmuted,
            elapsed_seconds: 0.0,
            duration_seconds: 0.0,
            gapless_mode: Enabled,
            output_mode: Resampled,
        }
    }
}

/// Playback status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    /// Actively playing.
    Playing,
    /// Paused.
    Paused,
    /// Stopped.
    Stopped,
}

/// Check whether a playback event can change the signal path.
///
/// Track, format, setting, device, and status changes rebuild the snapshot;
/// position ticks, queue edits, seeks, gapless toggles, and errors never do.
///
/// # Arguments
///
/// * `event` - Engine event to classify.
///
/// # Returns
///
/// * `bool` - Whether the signal path needs a rebuild.
#[must_use]
pub const fn snapshot_event(event: &PlaybackEvent) -> bool {
    matches!(
        event,
        PlaybackEvent::TrackStarted { .. }
            | PlaybackEvent::TrackFormatReady { .. }
            | PlaybackEvent::TrackFinished { .. }
            | PlaybackEvent::Paused
            | PlaybackEvent::Resumed
            | PlaybackEvent::Stopped
            | PlaybackEvent::VolumeChanged { .. }
            | PlaybackEvent::OutputModeChanged { .. }
            | PlaybackEvent::DeviceLost { .. }
    )
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::playback::{
        devices::OutputMode::{BitPerfect, Resampled},
        state::{
            PlaybackEvent::{
                DeviceLost, Error, GaplessEnabledChanged, OutputModeChanged, Paused, PositionTick,
                QueueChanged, Resumed, Seeked, Stopped, TrackFinished, TrackFormatReady,
                TrackStarted, VolumeChanged,
            },
            snapshot_event,
        },
    };

    #[test]
    fn snapshot_event_classifies_path_changes() -> Result<()> {
        let rebuilds = [
            TrackStarted { track_id: 1 },
            TrackFormatReady {
                track_id: 1,
                sample_rate: 48000,
            },
            TrackFinished { track_id: 1 },
            Paused,
            Resumed,
            Stopped,
            VolumeChanged { volume: 0.5 },
            OutputModeChanged { mode: Resampled },
            OutputModeChanged { mode: BitPerfect },
            DeviceLost {
                error: String::from("unplugged"),
            },
        ];
        for event in &rebuilds {
            let rebuild = snapshot_event(event);
            ensure!(rebuild, "rebuilds classify");
        }
        let ignored = [
            PositionTick {
                elapsed_seconds: 1.0,
                duration_seconds: 200.0,
            },
            QueueChanged { track_ids: vec![] },
            Seeked {
                position_seconds: 10.0,
            },
            Error {
                error: String::from("decode failed"),
            },
            GaplessEnabledChanged { enabled: true },
        ];
        for event in &ignored {
            let silent = snapshot_event(event);
            ensure!(!silent, "others never rebuild");
        }
        Ok(())
    }
}

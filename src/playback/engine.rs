//! Playback orchestrator wiring decoder, resampler, ring buffer, and output together.
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use {async_channel::Sender, parking_lot::Mutex, tokio::sync::mpsc::Sender as MpscSender};

use crate::playback::{
    gapless::{
        GaplessMode::{self, Enabled},
        GaplessTransitioner,
    },
    output::{
        AudioOutput,
        OutputMode::{self, Resampled},
    },
    queue::PlaybackQueue,
};

/// Commands sent to the decode task.
pub enum DecodeCommand {
    /// Seek to a position in seconds.
    Seek(f64),
    /// Pause the audio output stream.
    Pause,
    /// Resume the audio output stream.
    Resume,
    /// Pre-buffer the next track for gapless transition.
    PreloadNext {
        /// ID of the next track.
        track_id: i64,
        /// File path of the next track.
        path: PathBuf,
    },
}

/// Shared engine state.
pub struct EngineShared {
    /// Current playback state.
    pub state: Mutex<PlaybackState>,
    /// Playback queue.
    pub queue: PlaybackQueue,
    /// Per-subscriber event senders for fan-out broadcast.
    pub event_subs: Mutex<Vec<Sender<PlaybackEvent>>>,
    /// Command sender for the active decode task.
    pub decode_tx: Mutex<Option<MpscSender<DecodeCommand>>>,
    /// Join handle for the active decode thread.
    pub decode_thread: Mutex<Option<JoinHandle<()>>>,
    /// Active audio output kept alive during playback.
    pub output: Mutex<Option<AudioOutput>>,
    /// Cached track ID to file path mappings (set before `play_queue`).
    pub track_paths: Mutex<HashMap<i64, PathBuf>>,
    /// Device output sample rate, updated when `AudioOutput` is created.
    pub device_sample_rate: Mutex<u32>,
    /// Current track sample rate, updated on track start.
    pub track_sample_rate: Mutex<u32>,
    /// Shared flag set when the audio device is lost.
    pub device_lost: Arc<AtomicBool>,
    /// Gapless transitioner for seamless track transitions.
    pub transitioner: Mutex<GaplessTransitioner>,
}

impl EngineShared {
    /// Broadcast an event to all subscribers, removing closed channels.
    pub fn send_event(&self, event: &PlaybackEvent) {
        let mut subs = self.event_subs.lock();
        subs.retain(|tx| tx.try_send(event.clone()).is_ok());
    }

    /// Send an error event to all subscribers.
    pub fn send_error_event(&self, error: &str) {
        self.send_event(&PlaybackEvent::Error {
            error: error.to_string(),
        });
    }

    /// Update elapsed seconds and optionally emit a position tick.
    pub fn update_elapsed(&self, elapsed: f64, last_tick: &mut Instant) {
        let mut state = self.state.lock();
        state.elapsed_seconds = elapsed;
        if last_tick.elapsed() >= Duration::from_millis(200) {
            let duration = state.duration_seconds;
            drop(state);
            self.send_event(&PlaybackEvent::PositionTick {
                elapsed_seconds: elapsed,
                duration_seconds: duration,
            });
            *last_tick = Instant::now();
        }
    }
}

impl Default for EngineShared {
    fn default() -> Self {
        Self {
            state: Mutex::new(PlaybackState::default()),
            queue: PlaybackQueue::new(),
            event_subs: Mutex::new(Vec::new()),
            decode_tx: Mutex::new(None),
            decode_thread: Mutex::new(None),
            output: Mutex::new(None),
            track_paths: Mutex::new(HashMap::new()),
            device_sample_rate: Mutex::new(44100),
            track_sample_rate: Mutex::new(44100),
            device_lost: Arc::new(AtomicBool::new(false)),
            transitioner: Mutex::new(GaplessTransitioner::new()),
        }
    }
}

/// Mute state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteState {
    /// Audio is muted.
    Muted,
    /// Audio is unmuted.
    Unmuted,
}

/// Playback engine orchestrator.
#[derive(Default)]
pub struct PlaybackEngine {
    /// Shared state wrapped in an `Arc`.
    pub shared: Arc<EngineShared>,
}

impl PlaybackEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(EngineShared::default()),
        }
    }

    /// Pre-load track ID to file path mappings for queue navigation.
    pub fn set_track_paths(&self, paths: HashMap<i64, PathBuf>) {
        *self.shared.track_paths.lock() = paths;
    }

    /// Returns a reference to the playback queue.
    #[must_use]
    pub fn queue(&self) -> &PlaybackQueue {
        &self.shared.queue
    }

    /// Set `current_album_id` if `track_id` matches the currently playing track.
    pub fn set_album_id_if_current(&self, track_id: i64, album_id: i64) {
        let mut state = self.shared.state.lock();
        if Some(track_id) == state.current_track_id {
            state.current_album_id = album_id;
        }
    }

    /// Reset `current_album_id` to `-1`.
    pub fn reset_album_id(&self) {
        self.shared.state.lock().current_album_id = -1;
    }
}

/// Events emitted by the playback engine.
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    /// A track started playing.
    TrackStarted {
        /// Track ID.
        track_id: i64,
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use anyhow::{Result, anyhow, bail};

    use crate::playback::{
        PlaybackError::{NoDeviceAvailable, Output, QueueEmpty, TrackNotFound},
        control::PlaybackController,
        engine::{PlaybackEngine, PlaybackStatus::Stopped},
    };

    fn setup_queue(engine: &PlaybackEngine, track_ids: Vec<i64>) {
        let paths: HashMap<_, _> = track_ids
            .iter()
            .map(|id| (*id, PathBuf::from(format!("/fake/{id}.flac"))))
            .collect();
        engine.set_track_paths(paths);
        engine.queue().set_queue(track_ids);
    }

    #[test]
    fn engine_creates_with_default_state() {
        let engine = PlaybackEngine::new();
        let state = engine.state();
        assert_eq!(state.status, Stopped);
        assert!((state.volume - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_volume_clamps() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.set_volume(2.0).map_err(|e| anyhow!("{e}"))?;
        if (engine.state().volume - 1.0).abs() >= f64::EPSILON {
            bail!("volume should be clamped to 1.0");
        }
        engine.set_volume(-0.5).map_err(|e| anyhow!("{e}"))?;
        if engine.state().volume.abs() >= f64::EPSILON {
            bail!("volume should be clamped to 0.0");
        }
        Ok(())
    }

    #[test]
    fn stop_when_not_playing_is_noop() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.stop().map_err(|e| anyhow!("{e}"))?;
        if engine.state().status != Stopped {
            bail!("engine should not be playing after stop");
        }
        Ok(())
    }

    #[test]
    fn toggle_pause_when_not_playing_is_noop() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.toggle_pause().map_err(|e| anyhow!("{e}"))?;
        if engine.state().status != Stopped {
            bail!("engine should not be playing after toggle_pause");
        }
        Ok(())
    }

    #[test]
    fn play_queue_returns_error_when_empty() {
        let engine = PlaybackEngine::new();
        assert!(matches!(engine.play_queue(vec![]), Err(QueueEmpty)));
    }

    #[test]
    fn play_queue_returns_error_when_path_not_set() {
        let engine = PlaybackEngine::new();
        assert!(matches!(
            engine.play_queue(vec![42]),
            Err(TrackNotFound(42))
        ));
    }

    #[test]
    fn play_queue_succeeds_when_path_is_set() -> Result<()> {
        let engine = PlaybackEngine::new();
        setup_queue(&engine, vec![1, 2, 3]);
        let result = engine.play_queue(vec![1, 2, 3]);
        match result {
            Err(NoDeviceAvailable | Output(_)) | Ok(()) => Ok(()),
            Err(e) => bail!("unexpected error: {e}"),
        }
    }

    #[test]
    fn play_track_returns_error_when_not_found() {
        let engine = PlaybackEngine::new();
        assert!(matches!(engine.play_track(99), Err(TrackNotFound(99))));
    }

    #[test]
    fn next_track_returns_error_when_path_not_set() {
        let engine = PlaybackEngine::new();
        assert!(matches!(
            engine.play_queue(vec![1, 2]),
            Err(TrackNotFound(1))
        ));
        assert!(matches!(engine.next_track(), Err(TrackNotFound(2))));
    }

    #[test]
    fn next_track_returns_queue_empty_when_single() {
        let engine = PlaybackEngine::new();
        setup_queue(&engine, vec![1]);
        assert!(matches!(engine.next_track(), Err(QueueEmpty)));
    }

    #[test]
    fn previous_track_returns_error_at_start() {
        let engine = PlaybackEngine::new();
        assert!(matches!(engine.previous_track(), Err(QueueEmpty)));
    }
}

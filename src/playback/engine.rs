//! Playback orchestrator wiring decoder, ring buffer, and output together.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    thread::{spawn, yield_now},
};

use {
    parking_lot::Mutex,
    rtrb::{Producer, PushError::Full},
    tokio::sync::{
        broadcast::{Receiver, Sender, channel},
        mpsc::{
            Receiver as MpscReceiver, Sender as MpscSender, channel as MpscChannel,
            error::TryRecvError::{Disconnected, Empty},
        },
    },
    tracing::warn,
};

use crate::playback::{
    PlaybackError::{self, Output},
    decoder::{DecodedSamples, Decoder},
    output::AudioOutput,
    queue::PlaybackQueue,
};

/// Commands sent to the decode task.
enum DecodeCommand {
    /// Stop decoding and exit the loop.
    Stop,
    /// Device was lost — stop gracefully.
    DeviceLost,
}

/// Shared engine state.
struct EngineShared {
    /// Current playback state.
    state: Mutex<PlaybackState>,
    /// Playback queue.
    queue: PlaybackQueue,
    /// Broadcast sender for playback events.
    event_tx: Sender<PlaybackEvent>,
    /// Keep-alive receiver to prevent broadcast channel from closing.
    _event_rx: Receiver<PlaybackEvent>,
    /// Command sender for the active decode task.
    decode_tx: Mutex<Option<MpscSender<DecodeCommand>>>,
    /// Active audio output kept alive during playback.
    output: Mutex<Option<AudioOutput>>,
    /// Cached track ID to file path mappings (set before `play_queue`).
    track_paths: Mutex<HashMap<i64, PathBuf>>,
}

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
    fn set_volume(&self, volume: f32) -> Result<(), PlaybackError>;

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
}

/// The playback engine orchestrator.
///
/// Wires decoder → ring buffer → output. Manages the decode task lifecycle.
pub struct PlaybackEngine {
    /// Shared state wrapped in an `Arc`.
    shared: Arc<EngineShared>,
}

impl PlaybackEngine {
    /// Create a new playback engine.
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, event_rx) = channel(64);
        Self {
            shared: Arc::new(EngineShared {
                state: Mutex::new(PlaybackState::default()),
                queue: PlaybackQueue::new(),
                event_tx,
                _event_rx: event_rx,
                decode_tx: Mutex::new(None),
                output: Mutex::new(None),
                track_paths: Mutex::new(HashMap::new()),
            }),
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

    /// Start decoding and playing a track.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if output device cannot be opened.
    fn start_playback(&self, track_id: i64, path: PathBuf) -> Result<(), PlaybackError> {
        self.stop_decode_task();

        let ring_capacity = 48000 * 2;
        let (output, producer) = AudioOutput::open(ring_capacity)
            .inspect_err(|e| {
                send_error_event(
                    &self.shared.event_tx,
                    format!("Audio device unavailable: {e}"),
                );
            })
            .map_err(Output)?;
        *self.shared.output.lock() = Some(output);

        {
            let mut state = self.shared.state.lock();
            state.current_track_id = Some(track_id);
            state.current_path = Some(path.clone());
            state.is_playing = true;
            state.is_paused = false;
        }

        let engine_state = Arc::clone(&self.shared);
        let (cmd_tx, cmd_rx) = MpscChannel::<DecodeCommand>(4);

        *self.shared.decode_tx.lock() = Some(cmd_tx);

        let event_tx = self.shared.event_tx.clone();
        if let Err(e) = event_tx.send(PlaybackEvent::TrackStarted { track_id }) {
            warn!(error = %e, "Failed to send TrackStarted event");
        }

        spawn(move || {
            run_decode_loop(&path, producer, cmd_rx, &engine_state, &event_tx, track_id);
        });

        Ok(())
    }

    /// Stop the currently running decode task.
    fn stop_decode_task(&self) {
        if let Some(tx) = self.shared.decode_tx.lock().take() {
            drop(tx.try_send(DecodeCommand::Stop));
        }
        *self.shared.output.lock() = None;
    }
}

impl Default for PlaybackEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackController for PlaybackEngine {
    fn play_track(&self, track_id: i64) -> Result<(), PlaybackError> {
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&track_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(track_id))?;
        self.start_playback(track_id, path)
    }

    fn play_queue(&self, queue: Vec<i64>) -> Result<(), PlaybackError> {
        if queue.is_empty() {
            return Err(PlaybackError::QueueEmpty);
        }
        self.shared.queue.set_queue(queue);
        let first_id = self
            .shared
            .queue
            .current()
            .ok_or(PlaybackError::QueueEmpty)?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&first_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(first_id))?;
        self.start_playback(first_id, path)
    }

    fn toggle_pause(&self) -> Result<(), PlaybackError> {
        let mut state = self.shared.state.lock();
        if !state.is_playing {
            return Ok(());
        }
        let event = if state.is_paused {
            state.is_paused = false;
            PlaybackEvent::Resumed
        } else {
            state.is_paused = true;
            PlaybackEvent::Paused
        };
        drop(state);
        if let Err(e) = self.shared.event_tx.send(event) {
            warn!(error = %e, "Failed to send pause toggle event");
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        self.stop_decode_task();
        let mut state = self.shared.state.lock();
        state.is_playing = false;
        state.is_paused = false;
        state.current_track_id = None;
        state.current_path = None;
        drop(state);
        if let Err(e) = self.shared.event_tx.send(PlaybackEvent::Stopped) {
            warn!(error = %e, "Failed to send Stopped event");
        }
        Ok(())
    }

    fn next_track(&self) -> Result<(), PlaybackError> {
        let next_id = self.shared.queue.next().ok_or(PlaybackError::QueueEmpty)?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&next_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(next_id))?;
        self.start_playback(next_id, path)
    }

    fn previous_track(&self) -> Result<(), PlaybackError> {
        let prev_id = self
            .shared
            .queue
            .previous()
            .ok_or(PlaybackError::QueueEmpty)?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&prev_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(prev_id))?;
        self.start_playback(prev_id, path)
    }

    fn set_volume(&self, volume: f32) -> Result<(), PlaybackError> {
        let clamped = volume.clamp(0.0, 1.0);
        self.shared.state.lock().volume = clamped;
        if let Err(e) = self.shared.event_tx.send(PlaybackEvent::VolumeChanged {
            volume: f64::from(clamped),
        }) {
            warn!(error = %e, "Failed to send VolumeChanged event");
        }
        Ok(())
    }

    fn set_muted(&self, muted: bool) -> Result<(), PlaybackError> {
        self.shared.state.lock().is_muted = muted;
        Ok(())
    }

    fn subscribe(&self) -> Receiver<PlaybackEvent> {
        self.shared.event_tx.subscribe()
    }

    fn state(&self) -> PlaybackState {
        self.shared.state.lock().clone()
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
}

/// Current state of the playback engine.
#[derive(Debug, Clone)]
pub struct PlaybackState {
    /// The currently playing track ID, if any.
    pub current_track_id: Option<i64>,
    /// The file path of the currently playing track, if any.
    pub current_path: Option<PathBuf>,
    /// Whether the engine is currently playing.
    pub is_playing: bool,
    /// Whether the engine is paused.
    pub is_paused: bool,
    /// Current volume (0.0 to 1.0).
    pub volume: f32,
    /// Whether audio is muted.
    pub is_muted: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            current_track_id: None,
            current_path: None,
            is_playing: false,
            is_paused: false,
            volume: 1.0,
            is_muted: false,
        }
    }
}

/// Path resolver: maps track IDs to file paths.
pub trait TrackPathResolver: Send + Sync + 'static {
    /// Resolve a track ID to its file path.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if the track ID cannot be resolved.
    fn resolve(&self, track_id: i64) -> Result<PathBuf, PlaybackError>;
}

/// Read the current volume from shared state.
fn read_volume(shared: &EngineShared) -> f32 {
    let state = shared.state.lock();
    if state.is_muted { 0.0 } else { state.volume }
}

/// Send an error event through the broadcast channel.
fn send_error_event(event_tx: &Sender<PlaybackEvent>, error: String) {
    if let Err(e) = event_tx.send(PlaybackEvent::Error { error }) {
        warn!(error = %e, "Failed to send error event");
    }
}

/// The decode loop running on a blocking thread.
fn run_decode_loop(
    path: &Path,
    mut producer: Producer<f32>,
    mut cmd_rx: MpscReceiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    event_tx: &Sender<PlaybackEvent>,
    track_id: i64,
) {
    let mut decoder = match Decoder::open(path) {
        Ok(d) => d,
        Err(e) => return send_error_event(event_tx, e.to_string()),
    };

    let mut volume;
    let mut event_to_send = None;

    loop {
        volume = read_volume(engine_shared);

        match cmd_rx.try_recv() {
            Err(Empty) => {}
            Ok(DecodeCommand::Stop | DecodeCommand::DeviceLost) | Err(Disconnected) => break,
        }

        match decoder.decode_next() {
            Ok(batch) if batch.samples.is_empty() => {
                event_to_send = Some(PlaybackEvent::TrackFinished { track_id });
                break;
            }
            Ok(batch) => {
                push_samples(&batch, &mut producer, volume);
            }
            Err(e) => {
                event_to_send = Some(PlaybackEvent::Error {
                    error: e.to_string(),
                });
                break;
            }
        }
    }

    {
        let mut state = engine_shared.state.lock();
        state.is_playing = false;
        state.is_paused = false;
        state.current_track_id = None;
        state.current_path = None;
    }
    *engine_shared.decode_tx.lock() = None;
    if let Some(event) = event_to_send
        && let Err(e) = event_tx.send(event)
    {
        warn!(error = %e, "Failed to send playback event");
    }
    if let Err(e) = event_tx.send(PlaybackEvent::Stopped) {
        warn!(error = %e, "Failed to send final Stopped event");
    }
}

/// Push samples from a decoded batch into the ring buffer.
///
/// Blocks by yielding the thread when the ring buffer is full, preventing
/// sample loss and throttling the decode loop to real-time playback rate.
fn push_samples(batch: &DecodedSamples, producer: &mut Producer<f32>, volume: f32) {
    for sample in &batch.samples {
        let mut s = *sample * volume;
        while let Err(ret) = producer.push(s) {
            s = match ret {
                Full(val) => val,
            };
            yield_now();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use anyhow::{Result, anyhow, bail};

    use crate::playback::{
        PlaybackError::{NoDeviceAvailable, Output, QueueEmpty, TrackNotFound},
        engine::{PlaybackController, PlaybackEngine},
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
        assert!(!state.is_playing);
        assert!(!state.is_paused);
        assert!((state.volume - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn set_volume_clamps() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.set_volume(2.0).map_err(|e| anyhow!("{e}"))?;
        if (engine.state().volume - 1.0).abs() >= f32::EPSILON {
            bail!("volume should be clamped to 1.0");
        }
        engine.set_volume(-0.5).map_err(|e| anyhow!("{e}"))?;
        if engine.state().volume.abs() >= f32::EPSILON {
            bail!("volume should be clamped to 0.0");
        }
        Ok(())
    }

    #[test]
    fn stop_when_not_playing_is_noop() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.stop().map_err(|e| anyhow!("{e}"))?;
        if engine.state().is_playing {
            bail!("engine should not be playing after stop");
        }
        Ok(())
    }

    #[test]
    fn toggle_pause_when_not_playing_is_noop() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.toggle_pause().map_err(|e| anyhow!("{e}"))?;
        if engine.state().is_playing {
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

//! Playback orchestrator wiring decoder, resampler, ring buffer, and output together.

use std::{
    collections::HashMap,
    iter::repeat_n,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread::{Builder, JoinHandle, sleep},
    time::{Duration, Instant},
};

use {
    async_channel::{Receiver, Sender, unbounded},
    parking_lot::Mutex,
    rtrb::{Producer, PushError::Full},
    tokio::sync::mpsc::{
        Receiver as MpscReceiver, Sender as MpscSender, channel as MpscChannel,
        error::TryRecvError::{Disconnected, Empty},
    },
    tracing::{error, info, warn},
};

use crate::playback::{
    PlaybackError,
    decoder::{DecodedSamples, Decoder},
    gapless::GaplessTransitioner,
    output::{
        AudioOutput,
        OutputMode::{self, BitPerfect, Resampled},
    },
    queue::PlaybackQueue,
    resampler::AudioResampler,
};

/// Commands sent to the decode task.
enum DecodeCommand {
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

/// Initialised decoder and resampler context for a decode loop.
struct DecoderCtx {
    /// Opened audio decoder.
    decoder: Decoder,
    /// Sample rate of the source track.
    track_sample_rate: u32,
    /// Number of channels in the source track.
    src_channels: usize,
    /// Optional resampler for sample-rate conversion.
    resampler: Option<AudioResampler>,
}

/// Shared engine state.
struct EngineShared {
    /// Current playback state.
    state: Mutex<PlaybackState>,
    /// Playback queue.
    queue: PlaybackQueue,
    /// Per-subscriber event senders for fan-out broadcast.
    event_subs: Mutex<Vec<Sender<PlaybackEvent>>>,
    /// Command sender for the active decode task.
    decode_tx: Mutex<Option<MpscSender<DecodeCommand>>>,
    /// Join handle for the active decode thread.
    decode_thread: Mutex<Option<JoinHandle<()>>>,
    /// Active audio output kept alive during playback.
    output: Mutex<Option<AudioOutput>>,
    /// Cached track ID to file path mappings (set before `play_queue`).
    track_paths: Mutex<HashMap<i64, PathBuf>>,
    /// Device output sample rate, updated when `AudioOutput` is created.
    device_sample_rate: Mutex<u32>,
    /// Current track sample rate, updated on track start.
    track_sample_rate: Mutex<u32>,
    /// Shared flag set when the audio device is lost.
    device_lost: Arc<AtomicBool>,
    /// Gapless transitioner for seamless track transitions.
    transitioner: Mutex<GaplessTransitioner>,
}

impl EngineShared {
    /// Broadcast an event to all subscribers, removing closed channels.
    fn send_event(&self, event: &PlaybackEvent) {
        let mut subs = self.event_subs.lock();
        subs.retain(|tx| tx.try_send(event.clone()).is_ok());
    }

    /// Update elapsed seconds and optionally emit a position tick.
    fn update_elapsed(&self, elapsed: f64, last_tick: &mut Instant) {
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

/// Gapless playback mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaplessMode {
    /// Gapless playback is enabled.
    Enabled,
    /// Gapless playback is disabled.
    Disabled,
}

/// Mutable decode loop state updated by gapless transitions.
struct LoopCtx {
    /// Active audio decoder.
    decoder: Decoder,
    /// Optional resampler for sample-rate conversion.
    resampler: Option<AudioResampler>,
    /// Sample rate of the current track.
    track_sample_rate: u32,
    /// Number of source channels in the current track.
    src_channels: usize,
    /// Track sample rate as f64 (for elapsed time calculation).
    track_sample_rate_f64: f64,
    /// Elapsed playback time in seconds.
    elapsed: f64,
    /// Last tick time for position update throttling.
    last_tick: Instant,
}

/// Mute state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteState {
    /// Audio is muted.
    Muted,
    /// Audio is unmuted.
    Unmuted,
}

/// Audio output configuration for the decode loop.
#[derive(Clone, Copy)]
struct OutputConfig {
    /// Device sample rate in Hz.
    device_sample_rate: u32,
    /// Number of output channels.
    channels: u16,
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

/// The playback engine orchestrator.
///
/// Wires decoder → resampler → ring buffer → output. Manages the decode
/// task lifecycle, sample rate reconfiguration, and bit-perfect output.
pub struct PlaybackEngine {
    /// Shared state wrapped in an `Arc`.
    shared: Arc<EngineShared>,
}

impl PlaybackEngine {
    /// Create a new playback engine.
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

    /// Start decoding and playing a track with optional resampling.
    ///
    /// Spawns a decode thread that handles its own `AudioOutput` lifecycle,
    /// keeping potentially-blocking device operations off the main thread.
    fn start_playback(&self, track_id: i64, path: PathBuf) {
        self.stop_decode_task();

        {
            let mut state = self.shared.state.lock();
            state.current_track_id = Some(track_id);
            state.current_album_id = -1;
            state.current_path = Some(path.clone());
            state.status = PlaybackStatus::Playing;
            state.elapsed_seconds = 0.0;
            state.duration_seconds = 0.0;
        }

        let engine_state = Arc::clone(&self.shared);
        let (cmd_tx, cmd_rx) = MpscChannel::<DecodeCommand>(4);

        *self.shared.decode_tx.lock() = Some(cmd_tx);

        info!(
            track_id,
            path = %path.display(),
            "Playback started",
        );

        self.shared
            .send_event(&PlaybackEvent::TrackStarted { track_id });

        let thread_name = format!("decode-{track_id}");
        match Builder::new().name(thread_name).spawn(move || {
            init_decode_thread(&path, cmd_rx, &engine_state, track_id);
        }) {
            Ok(handle) => *self.shared.decode_thread.lock() = Some(handle),
            Err(e) => {
                error!(error = %e, "Failed to spawn decode thread");
            }
        }

        let upcoming = self.shared.queue.upcoming();
        if let Some(next_id) = upcoming.first().copied()
            && let Some(next_path) = self.shared.track_paths.lock().get(&next_id).cloned()
            && let Some(tx) = self.shared.decode_tx.lock().as_ref()
            && let Err(e) = tx.try_send(DecodeCommand::PreloadNext {
                track_id: next_id,
                path: next_path,
            })
        {
            warn!(error = %e, "Failed to send PreloadNext command");
        }
    }

    /// Stop the currently running decode task.
    ///
    /// Drops the command sender so the old decode thread sees `Disconnected`
    /// and exits. Does NOT drop the audio output — the new decode thread
    /// handles that to keep potentially-blocking `Stream::drop` off the
    /// main thread.
    fn stop_decode_task(&self) {
        if let Some(tx) = self.shared.decode_tx.lock().take() {
            drop(tx);
        }
    }
}

/// Crate-internal helpers on `PlaybackEngine`.
impl PlaybackEngine {
    /// Set `current_album_id` if `track_id` matches the currently playing track.
    ///
    /// Both the check and the write happen under the state lock, making this
    /// atomic w.r.t. `start_playback` / `stop`.
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
            .ok_or_else(|| {
                warn!(track_id, "Track not found for playback",);
                PlaybackError::TrackNotFound(track_id)
            })?;
        info!(track_id, "Play track command",);
        self.start_playback(track_id, path);
        Ok(())
    }

    fn play_queue(&self, queue: Vec<i64>) -> Result<(), PlaybackError> {
        if queue.is_empty() {
            warn!(
                queue_len = queue.len(),
                "Play queue command with empty queue"
            );
            return Err(PlaybackError::QueueEmpty);
        }
        let queue_len = queue.len();
        info!(queue_len, "Play queue command",);
        self.shared.queue.set_queue(queue.clone());
        self.shared
            .send_event(&PlaybackEvent::QueueChanged { track_ids: queue });
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
        self.start_playback(first_id, path);
        Ok(())
    }

    fn toggle_pause(&self) -> Result<(), PlaybackError> {
        let is_playing = {
            let state = self.shared.state.lock();
            state.status != PlaybackStatus::Stopped
        };
        if !is_playing {
            info!("Toggle pause ignored — not playing");
            return Ok(());
        }

        let mut state = self.shared.state.lock();
        let was_paused = state.status == PlaybackStatus::Paused;
        let track_id = state.current_track_id;
        let (event, cmd) = if was_paused {
            state.status = PlaybackStatus::Playing;
            info!(track_id, "Playback resumed");
            (PlaybackEvent::Resumed, DecodeCommand::Resume)
        } else {
            state.status = PlaybackStatus::Paused;
            info!(track_id, "Playback paused");
            (PlaybackEvent::Paused, DecodeCommand::Pause)
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
        self.stop_decode_task();
        let mut state = self.shared.state.lock();
        state.status = PlaybackStatus::Stopped;
        state.current_track_id = None;
        state.current_path = None;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
        drop(state);
        self.shared.send_event(&PlaybackEvent::Stopped);
        Ok(())
    }

    fn next_track(&self) -> Result<(), PlaybackError> {
        let next_id = self.shared.queue.next().ok_or_else(|| {
            info!("Next track failed — queue empty");
            PlaybackError::QueueEmpty
        })?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&next_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(next_id))?;
        self.start_playback(next_id, path);
        Ok(())
    }

    fn previous_track(&self) -> Result<(), PlaybackError> {
        let prev_id = self.shared.queue.previous().ok_or_else(|| {
            info!("Previous track failed — queue empty");
            PlaybackError::QueueEmpty
        })?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&prev_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(prev_id))?;
        self.start_playback(prev_id, path);
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
        self.shared
            .send_event(&PlaybackEvent::VolumeChanged { volume: clamped });
        Ok(())
    }

    fn set_muted(&self, muted: bool) -> Result<(), PlaybackError> {
        let vol = self.shared.state.lock().volume;
        let new_state = if muted {
            MuteState::Muted
        } else {
            MuteState::Unmuted
        };
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
        self.shared
            .send_event(&PlaybackEvent::OutputModeChanged { mode });
        Ok(())
    }

    fn set_gapless_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        info!(enabled, "Gapless playback toggled",);
        self.shared.state.lock().gapless_mode = if enabled {
            GaplessMode::Enabled
        } else {
            GaplessMode::Disabled
        };
        self.shared
            .send_event(&PlaybackEvent::GaplessEnabledChanged { enabled });
        Ok(())
    }

    fn seek_to(&self, position_seconds: f64) -> Result<(), PlaybackError> {
        let clamped = {
            let state = self.shared.state.lock();
            position_seconds.clamp(0.0, state.duration_seconds)
        };
        let cmd_tx = self.shared.decode_tx.lock();
        if let Some(tx) = cmd_tx.as_ref()
            && tx.try_send(DecodeCommand::Seek(clamped)).is_err()
        {}
        drop(cmd_tx);
        self.shared.state.lock().elapsed_seconds = clamped;
        self.shared.send_event(&PlaybackEvent::Seeked {
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
            gapless_mode: GaplessMode::Enabled,
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

/// Path resolver: maps track IDs to file paths.
pub trait TrackPathResolver: Send + Sync + 'static {
    /// Resolve a track ID to its file path.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if the track ID cannot be resolved.
    fn resolve(&self, track_id: i64) -> Result<PathBuf, PlaybackError>;
}

/// Send an error event to all subscribers.
fn send_error_event(subs: &Mutex<Vec<Sender<PlaybackEvent>>>, error: &str) {
    let mut subs = subs.lock();
    subs.retain(|tx| {
        tx.try_send(PlaybackEvent::Error {
            error: error.to_string(),
        })
        .is_ok()
    });
}

/// Attempt to reconnect the audio output after a device loss.
///
/// Drops the old output before opening a new one to avoid ALSA device
/// contention.
///
/// # Errors
///
/// Returns `Err(())` if a new audio output cannot be opened.
fn reconnect_device(engine_shared: &Arc<EngineShared>) -> Result<Producer<f32>, ()> {
    let err_msg = "Audio device disconnected during playback".to_string();
    warn!(error = %err_msg, "Audio device lost, attempting reconnection");
    engine_shared.send_event(&PlaybackEvent::DeviceLost { error: err_msg });

    *engine_shared.output.lock() = None;

    let ring_capacity = 48000 * 2;
    match AudioOutput::open(ring_capacity, &engine_shared.device_lost) {
        Ok((mut new_output, new_producer)) => {
            let state = engine_shared.state.lock();
            let current_vol = state.volume;
            let mode = state.output_mode;
            drop(state);
            if mode == BitPerfect {
                new_output.set_mode(mode);
                new_output.set_hardware_volume(current_vol);
            } else {
                new_output.set_volume_atomic(current_vol);
            }
            let sr = new_output.sample_rate();
            *engine_shared.device_sample_rate.lock() = sr;
            *engine_shared.output.lock() = Some(new_output);
            info!(
                sample_rate = sr,
                "Audio device reconnected, resuming playback"
            );
            engine_shared.send_event(&PlaybackEvent::Resumed);
            Ok(new_producer)
        }
        Err(e) => {
            error!(error = %e, "Audio device reconnection failed");
            send_error_event(
                &engine_shared.event_subs,
                &format!("Audio device reconnection failed: {e}"),
            );
            Err(())
        }
    }
}

/// Downmix interleaved frames from `src_channels` to fewer `dst_channels`
/// by averaging channel groups.
fn downsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let frames = samples.len() / src_channels;
    let mut out = Vec::with_capacity(frames * dst_channels);
    for frame in samples.chunks_exact(src_channels) {
        for out_ch in 0..dst_channels {
            let start_ch = (out_ch * src_channels) / dst_channels;
            let end_ch = ((out_ch + 1) * src_channels) / dst_channels;
            let count = u8::try_from(end_ch - start_ch).unwrap_or(1);
            out.push(frame[start_ch..end_ch].iter().sum::<f32>() / f32::from(count));
        }
    }
    out
}

/// Upmix interleaved frames from `src_channels` to more `dst_channels` by
/// padding extra channels with silence.
fn upsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let pad = dst_channels - src_channels;
    let frames = samples.len() / src_channels;
    let mut out = Vec::with_capacity(frames * dst_channels);
    for frame in samples.chunks_exact(src_channels) {
        out.extend_from_slice(frame);
        out.extend(repeat_n(0.0, pad));
    }
    out
}

/// Downmix interleaved samples from `src_channels` to `dst_channels`.
///
/// When `src_channels > dst_channels`, source channels are averaged into
/// groups to produce the output channels. When `src_channels < dst_channels`,
/// extra output channels are filled with silence.
fn downmix(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    if src_channels == dst_channels {
        return samples.to_vec();
    }
    if dst_channels > src_channels {
        upsample_frames(samples, src_channels, dst_channels)
    } else {
        downsample_frames(samples, src_channels, dst_channels)
    }
}

/// Return `batch.samples` as-is if channel counts match, otherwise downmix.
fn maybe_downmix(batch: DecodedSamples, src_channels: usize, dst_channels: usize) -> Vec<f32> {
    if src_channels == dst_channels {
        batch.samples
    } else {
        downmix(&batch.samples, src_channels, dst_channels)
    }
}

/// Loop pushing a single sample, retrying on full buffer.
///
/// Returns `false` if the producer is abandoned.
fn push_sample(sample: f32, producer: &mut Producer<f32>) -> bool {
    let mut s = sample;
    loop {
        match producer.push(s) {
            Ok(()) => return true,
            Err(Full(val)) => {
                s = val;
            }
        }
        if producer.is_abandoned() {
            return false;
        }
        sleep(Duration::from_millis(1));
    }
}

/// Push interleaved f32 samples into the ring buffer.
///
/// Volume scaling is now handled by the audio callback via an atomic,
/// so samples pass through unchanged here.
///
/// Blocks by yielding the thread when the ring buffer is full, preventing
/// sample loss and throttling the decode loop to real-time playback rate.
/// Returns early if the producer is abandoned (all consumers dropped).
fn push_samples(samples: &[f32], producer: &mut Producer<f32>) {
    for sample in samples {
        if !push_sample(*sample, producer) {
            return;
        }
    }
}

/// Pushes samples through a resampler and writes output to the ring buffer.
///
/// Returns `Some(error)` if resampling fails.
fn process_resampler(
    r: &mut AudioResampler,
    samples: &[f32],
    producer: &mut Producer<f32>,
) -> Option<String> {
    r.push_input(samples);
    while r.has_pending_output() {
        match r.process() {
            Ok(Some(output)) => push_samples(output, producer),
            Ok(None) => break,
            Err(e) => return Some(format!("Resampler error: {e}")),
        }
    }
    None
}

/// Process one decoded frame from the decoder.
///
/// Handles empty batches (track finished with possible gapless transition),
/// normal decoded batches, and decode errors. Returns `true` if the caller
/// should exit the decode loop.
fn process_decode_frame(
    ctx: &mut LoopCtx,
    engine_shared: &Arc<EngineShared>,
    event_to_send: &mut Option<PlaybackEvent>,
    producer: &mut Producer<f32>,
    output_cfg: OutputConfig,
    track_id: i64,
) -> bool {
    match ctx.decoder.decode_next() {
        Ok(batch)
            if batch.samples.is_empty()
                && handle_empty_batch(
                    engine_shared,
                    ctx,
                    event_to_send,
                    output_cfg.channels as usize,
                    output_cfg.device_sample_rate,
                ) =>
        {
            false
        }
        Ok(batch) if batch.samples.is_empty() => {
            *event_to_send = Some(PlaybackEvent::TrackFinished { track_id });
            true
        }
        Ok(batch) => {
            let frame_count =
                u32::try_from(batch.samples.len() / ctx.src_channels).unwrap_or(u32::MAX);
            ctx.elapsed += f64::from(frame_count) / ctx.track_sample_rate_f64;
            engine_shared.update_elapsed(ctx.elapsed, &mut ctx.last_tick);
            let samples = maybe_downmix(batch, ctx.src_channels, output_cfg.channels as usize);
            *event_to_send = process_decoded_batch(&samples, &mut ctx.resampler, producer);
            event_to_send.is_some() || producer.is_abandoned()
        }
        Err(e) => {
            *event_to_send = Some(PlaybackEvent::Error {
                error: e.to_string(),
            });
            true
        }
    }
}

/// Processes a decoded batch, optionally resampling, and returns an event if
/// an error occurred.
fn process_decoded_batch(
    samples: &[f32],
    resampler: &mut Option<AudioResampler>,
    producer: &mut Producer<f32>,
) -> Option<PlaybackEvent> {
    let error = if let Some(r) = resampler {
        process_resampler(r, samples, producer)
    } else {
        push_samples(samples, producer);
        None
    };
    error.map(|e| PlaybackEvent::Error { error: e })
}

/// Send a `TrackStarted` event and attempt to preload the next track.
fn send_track_started_and_preload_next(engine_shared: &Arc<EngineShared>, next_id: i64) {
    engine_shared.send_event(&PlaybackEvent::TrackStarted { track_id: next_id });

    let upcoming = engine_shared.queue.upcoming();
    if let Some(next_next_id) = upcoming.first().copied()
        && let Some(next_next_path) = engine_shared.track_paths.lock().get(&next_next_id).cloned()
        && let Some(tx) = engine_shared.decode_tx.lock().as_ref()
        && let Err(e) = tx.try_send(DecodeCommand::PreloadNext {
            track_id: next_next_id,
            path: next_next_path,
        })
    {
        warn!(error = %e, "Failed to send PreloadNext command");
    }
}

/// Try to advance to the next track in the queue after a track finishes.
///
/// Advances the queue, updates state, and spawns a new decode thread
/// (which handles its own `AudioOutput` lifecycle). Returns `true` if
/// the next track was started.
fn try_auto_advance(
    engine_shared: &Arc<EngineShared>,
    event_to_send: &mut Option<PlaybackEvent>,
) -> bool {
    let next_track = match &event_to_send {
        Some(PlaybackEvent::TrackFinished { .. }) => {
            let next_id = engine_shared.queue.next();
            next_id.and_then(|next_id| {
                let path = engine_shared.track_paths.lock().get(&next_id).cloned()?;
                Some((next_id, path))
            })
        }
        _ => None,
    };
    let Some((next_id, next_path)) = next_track else {
        return false;
    };

    *engine_shared.decode_tx.lock() = None;

    {
        let mut state = engine_shared.state.lock();
        state.current_track_id = Some(next_id);
        state.current_album_id = -1;
        state.current_path = Some(next_path.clone());
        state.status = PlaybackStatus::Playing;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
    }

    let (new_cmd_tx, new_cmd_rx) = MpscChannel(4);
    *engine_shared.decode_tx.lock() = Some(new_cmd_tx);

    info!(next_id, "Auto-advancing to next track",);

    if let Some(tf_event) = event_to_send.take() {
        engine_shared.send_event(&tf_event);
    }
    send_track_started_and_preload_next(engine_shared, next_id);

    let engine_state = Arc::clone(engine_shared);
    let thread_name = format!("decode-{next_id}");
    match Builder::new().name(thread_name).spawn(move || {
        init_decode_thread(&next_path, new_cmd_rx, &engine_state, next_id);
    }) {
        Ok(handle) => *engine_shared.decode_thread.lock() = Some(handle),
        Err(e) => error!(error = %e, "Failed to spawn decode thread"),
    }

    true
}

/// Attempt auto-advance or clean up playback state and emit final events.
fn finalize_track(engine_shared: &Arc<EngineShared>, event_to_send: &mut Option<PlaybackEvent>) {
    if try_auto_advance(engine_shared, event_to_send) {
        return;
    }

    {
        let mut state = engine_shared.state.lock();
        let had_track = state.current_track_id.is_some();
        state.status = PlaybackStatus::Stopped;
        state.current_track_id = None;
        state.current_path = None;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
        drop(state);
        if had_track
            && engine_shared.queue.upcoming().is_empty()
            && event_to_send
                .as_ref()
                .is_some_and(|e| matches!(e, PlaybackEvent::TrackFinished { .. }))
        {
            info!("Playback finished — queue empty, entering idle state");
        }
    }
    *engine_shared.decode_tx.lock() = None;
    if let Some(event) = event_to_send.take() {
        engine_shared.send_event(&event);
    }
    engine_shared.send_event(&PlaybackEvent::Stopped);
}

/// Open a decoder for `path` and create a resampler if needed.
///
/// Returns `None` on failure (error event sent via `engine_shared`).
fn init_decoder(
    path: &Path,
    engine_shared: &Arc<EngineShared>,
    output: OutputConfig,
) -> Option<DecoderCtx> {
    let decoder = match Decoder::open(path) {
        Ok(d) => d,
        Err(e) => {
            send_error_event(
                &engine_shared.event_subs,
                &format!("Failed to open decoder: {e}"),
            );
            return None;
        }
    };
    let track_sample_rate = decoder.params().sample_rate;
    let src_channels = decoder.params().channels as usize;
    let out_channels = output.channels as usize;

    *engine_shared.track_sample_rate.lock() = track_sample_rate;

    {
        let mut state = engine_shared.state.lock();
        state.duration_seconds = decoder.params().duration_seconds;
        state.elapsed_seconds = 0.0;
    }

    let resampler = if track_sample_rate == output.device_sample_rate {
        None
    } else {
        match AudioResampler::new(
            track_sample_rate,
            output.device_sample_rate,
            1024,
            out_channels,
        ) {
            Ok(r) => Some(r),
            Err(e) => {
                send_error_event(
                    &engine_shared.event_subs,
                    &format!("Failed to create resampler: {e}"),
                );
                return None;
            }
        }
    };

    Some(DecoderCtx {
        decoder,
        track_sample_rate,
        src_channels,
        resampler,
    })
}

/// Initialise the audio output on the decode thread, then run the decode loop.
///
/// Drops the previous audio output before opening a new one, keeping
/// potentially-blocking ALSA stream operations off the main thread.
fn init_decode_thread(
    path: &Path,
    cmd_rx: MpscReceiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    track_id: i64,
) {
    *engine_shared.output.lock() = None;

    let ring_capacity = 48000 * 2;
    let device_lost = Arc::clone(&engine_shared.device_lost);
    let (output, producer) = match AudioOutput::open(ring_capacity, &device_lost) {
        Ok(pair) => pair,
        Err(e) => {
            send_error_event(
                &engine_shared.event_subs,
                &format!("Audio device unavailable: {e}"),
            );
            return;
        }
    };

    let output_config = OutputConfig {
        device_sample_rate: output.sample_rate(),
        channels: output.channels(),
    };
    *engine_shared.device_sample_rate.lock() = output_config.device_sample_rate;
    let current_volume = engine_shared.state.lock().volume;
    if output.mode() == Resampled {
        output.set_volume_atomic(current_volume);
    }
    *engine_shared.output.lock() = Some(output);

    run_decode_loop(
        path,
        producer,
        cmd_rx,
        engine_shared,
        track_id,
        output_config,
    );
}

/// The decode loop running on a blocking thread.
///
/// Optionally uses a resampler when the track sample rate differs from
/// the device sample rate. Downmixes multichannel audio to the output
/// channel count to prevent playback speed distortion from channel count
/// mismatch between source and output.
fn run_decode_loop(
    path: &Path,
    mut producer: Producer<f32>,
    mut cmd_rx: MpscReceiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    track_id: i64,
    output: OutputConfig,
) {
    let Some(DecoderCtx {
        decoder,
        track_sample_rate,
        src_channels,
        resampler,
    }) = init_decoder(path, engine_shared, output)
    else {
        return;
    };

    let mut event_to_send = None;
    let mut ctx = LoopCtx {
        decoder,
        resampler,
        track_sample_rate,
        src_channels,
        track_sample_rate_f64: f64::from(track_sample_rate),
        elapsed: 0.0,
        last_tick: Instant::now(),
    };

    loop {
        if engine_shared.device_lost.load(Relaxed) {
            engine_shared.device_lost.store(false, Relaxed);
            match reconnect_device(engine_shared) {
                Ok(new_producer) => producer = new_producer,
                Err(()) => break,
            }
        }

        if handle_decode_cmd(&mut cmd_rx, engine_shared, &mut ctx) {
            break;
        }

        if engine_shared.state.lock().status == PlaybackStatus::Paused {
            sleep(Duration::from_millis(1));
            continue;
        }

        if process_decode_frame(
            &mut ctx,
            engine_shared,
            &mut event_to_send,
            &mut producer,
            output,
            track_id,
        ) {
            break;
        }
    }

    if producer.is_abandoned() || engine_shared.state.lock().current_track_id != Some(track_id) {
        return;
    }
    finalize_track(engine_shared, &mut event_to_send);
}

/// Handle a decode command from the control channel.
///
/// Returns `true` if the caller should exit the decode loop
/// (channel disconnected or error).
fn handle_decode_cmd(
    cmd_rx: &mut MpscReceiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    ctx: &mut LoopCtx,
) -> bool {
    match cmd_rx.try_recv() {
        Err(Disconnected) => true,
        Ok(DecodeCommand::Seek(pos)) => {
            engine_shared.output.lock().as_ref().map(AudioOutput::flush);
            let actual = ctx.decoder.seek_to(pos).unwrap_or(pos);
            ctx.elapsed = actual;
            engine_shared.state.lock().elapsed_seconds = actual;
            false
        }
        Ok(DecodeCommand::Pause) => {
            engine_shared.output.lock().as_ref().map(AudioOutput::pause);
            false
        }
        Ok(DecodeCommand::Resume) => {
            engine_shared.output.lock().as_ref().map(AudioOutput::play);
            false
        }
        Ok(DecodeCommand::PreloadNext {
            track_id: next_id,
            path: next_path,
            ..
        }) => {
            let current = engine_shared.state.lock().current_track_id;
            if let Some(current) = current
                && let Err(e) = engine_shared
                    .transitioner
                    .lock()
                    .prebuffer_next(current, next_id, next_path)
            {
                warn!(error = %e, "Failed to pre-buffer next track");
            }
            false
        }
        Err(Empty) => false,
    }
}

/// Handle an empty decode batch (track finished).
///
/// Attempts a gapless transition. Returns `true` if a transition was applied
/// and the decode loop should continue. Returns `false` if no pre-buffered
/// track is available.
fn handle_empty_batch(
    engine_shared: &Arc<EngineShared>,
    ctx: &mut LoopCtx,
    event_to_send: &mut Option<PlaybackEvent>,
    dst_channels: usize,
    device_sample_rate: u32,
) -> bool {
    let mut transitioner = engine_shared.transitioner.lock();
    let next_id = transitioner.next_track_id();
    let next_decoder = transitioner.transition();
    drop(transitioner);

    let (Some(next_id), Some(next_decoder)) = (next_id, next_decoder) else {
        return false;
    };

    let params = next_decoder.params();
    let next_sr = params.sample_rate;

    if let Some(advanced_id) = engine_shared.queue.next() {
        debug_assert_eq!(advanced_id, next_id, "Queue advanced to unexpected track");
    }
    {
        let mut state = engine_shared.state.lock();
        state.current_track_id = Some(next_id);
        let path = engine_shared.track_paths.lock().get(&next_id).cloned();
        state.current_path = path;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = params.duration_seconds;
    }
    *engine_shared.track_sample_rate.lock() = next_sr;

    if next_sr != ctx.track_sample_rate {
        match AudioResampler::new(next_sr, device_sample_rate, 1024, dst_channels) {
            Ok(r) => ctx.resampler = Some(r),
            Err(e) => {
                *event_to_send = Some(PlaybackEvent::Error {
                    error: format!("Resampler reconfiguration failed: {e}"),
                });
                return false;
            }
        }
    }

    ctx.decoder = next_decoder;
    ctx.track_sample_rate = next_sr;
    ctx.src_channels = params.channels as usize;
    ctx.elapsed = 0.0;
    ctx.last_tick = Instant::now();
    ctx.track_sample_rate_f64 = f64::from(next_sr);

    send_track_started_and_preload_next(engine_shared, next_id);

    true
}

/// Create a new resampler for a given sample rate pair.
///
/// # Errors
///
/// Returns a descriptive error string if the resampler cannot be created.
pub fn create_resampler(
    input_rate: u32,
    output_rate: u32,
    channels: usize,
) -> Result<AudioResampler, String> {
    AudioResampler::new(input_rate, output_rate, 1024, channels)
        .map_err(|e| format!("Resampler creation failed: {e}"))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use anyhow::{Result, anyhow, bail};

    use crate::playback::{
        PlaybackError::{NoDeviceAvailable, Output, QueueEmpty, TrackNotFound},
        decoder::{AudioParams, DecodedSamples},
        engine::{
            EngineShared, PlaybackController, PlaybackEngine,
            PlaybackEvent::{Paused, TrackFinished},
            PlaybackStatus::Stopped,
            downmix, maybe_downmix, try_auto_advance,
        },
    };

    fn assert_approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < f32::EPSILON, "{a} != {b}");
    }

    fn make_shared_engine() -> Arc<EngineShared> {
        Arc::new(EngineShared::default())
    }

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

    #[test]
    fn try_auto_advance_returns_false_for_non_track_finished() {
        let shared = make_shared_engine();
        let mut event = Some(Paused);
        let result = try_auto_advance(&shared, &mut event);
        assert!(!result, "should return false for non-TrackFinished event");
    }

    #[test]
    fn try_auto_advance_returns_false_when_no_upcoming_track() {
        let shared = make_shared_engine();
        shared.queue.set_queue(vec![1]);
        let mut event = Some(TrackFinished { track_id: 1 });
        let result = try_auto_advance(&shared, &mut event);
        assert!(!result, "should return false when queue has only one track");
    }

    #[test]
    fn try_auto_advance_returns_false_when_path_not_found() {
        let shared = make_shared_engine();
        shared.queue.set_queue(vec![1, 2]);
        let mut event = Some(TrackFinished { track_id: 1 });
        let result = try_auto_advance(&shared, &mut event);
        assert!(
            !result,
            "should return false when path not found for upcoming track"
        );
    }

    #[test]
    fn downmix_equal_channels_returns_copy() {
        let samples = vec![1.0, -0.5, 0.25, -1.0];
        let result = downmix(&samples, 2, 2);
        assert_eq!(result, samples);
    }

    #[test]
    fn downmix_upmix_mono_to_stereo_pads_with_silence() {
        let samples = vec![0.75, -0.25];
        let result = downmix(&samples, 1, 2);
        assert_eq!(result, vec![0.75, 0.0, -0.25, 0.0]);
    }

    #[test]
    fn downmix_upmix_stereo_to_51_pads_extra_channels() {
        let samples = vec![0.5, -0.5, 1.0, -1.0];
        let result = downmix(&samples, 2, 6);
        assert_eq!(
            result,
            vec![0.5, -0.5, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn downmix_downmix_stereo_to_mono_averages() {
        let samples = vec![0.8, 0.2, -0.6, -0.4];
        let result = downmix(&samples, 2, 1);
        assert_approx_eq(result[0], 0.5);
        assert_approx_eq(result[1], -0.5);
    }

    #[test]
    fn downmix_downmix_51_to_stereo_averages_groups() {
        let samples = vec![1.0, 0.5, 0.0, 0.0, -1.0, -0.5];
        let result = downmix(&samples, 6, 2);
        assert_approx_eq(result[0], 0.5);
        assert_approx_eq(result[1], -0.5);
    }

    #[test]
    fn downmix_downmix_7ch_to_3ch_distributes_evenly() {
        let samples = vec![1.0, 2.0, 10.0, 20.0, 100.0, 200.0, 0.5];
        let result = downmix(&samples, 7, 3);
        assert_approx_eq(result[0], 1.5);
        assert_approx_eq(result[1], 15.0);
        assert!((result[2] - 100.166_67).abs() < 0.001);
    }

    #[test]
    fn downmix_downmix_5ch_to_2ch_uneven_groups() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = downmix(&samples, 5, 2);
        assert_approx_eq(result[0], 1.5);
        assert_approx_eq(result[1], 4.0);
    }

    #[test]
    fn maybe_downmix_no_downmix_when_channels_match() {
        let batch = DecodedSamples {
            samples: vec![0.5, -0.5, 0.25, -0.25],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
            },
        };
        let result = maybe_downmix(batch, 2, 2);
        assert_eq!(result, vec![0.5, -0.5, 0.25, -0.25]);
    }

    #[test]
    fn maybe_downmix_downmixes_when_channels_differ() {
        let batch = DecodedSamples {
            samples: vec![0.8, 0.2],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
            },
        };
        let result = maybe_downmix(batch, 2, 1);
        assert_eq!(result.len(), 1);
        assert!((result[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn downmix_multiple_frames_preserves_frame_boundaries() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let result = downmix(&samples, 4, 2);
        assert_eq!(result.len(), 4);
        assert!((result[0] - 1.5).abs() < f32::EPSILON);
        assert!((result[1] - 3.5).abs() < f32::EPSILON);
        assert!((result[2] - 5.5).abs() < f32::EPSILON);
        assert!((result[3] - 7.5).abs() < f32::EPSILON);
    }

    #[test]
    fn downmix_empty_input_returns_empty_output() {
        let result = downmix(&[], 2, 1);
        assert!(result.is_empty());
        let result = downmix(&[], 1, 6);
        assert!(result.is_empty());
    }
}

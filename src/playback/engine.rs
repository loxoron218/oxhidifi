//! Playback orchestrator wiring decoder, resampler, ring buffer, and output together.

use std::{
    collections::HashMap,
    iter::repeat_n,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread::{spawn, yield_now},
};

use {
    num_traits::cast::FromPrimitive,
    parking_lot::Mutex,
    rtrb::{Producer, PushError::Full},
    tokio::sync::{
        broadcast::{Receiver, Sender, channel},
        mpsc::{
            Receiver as MpscReceiver, Sender as MpscSender, channel as MpscChannel,
            error::TryRecvError::{Disconnected, Empty},
        },
    },
    tracing::{error, info, warn},
};

use crate::playback::{
    PlaybackError::{self, Output},
    decoder::{DecodedSamples, Decoder},
    output::AudioOutput,
    queue::PlaybackQueue,
    resampler::AudioResampler,
};

/// Commands sent to the decode task.
enum DecodeCommand {
    /// Stop decoding and exit the loop.
    Stop,
    /// Device was lost — stop gracefully.
    DeviceLost,
    /// Pre-buffer the next track for gapless transition.
    PreloadNext {
        /// ID of the next track.
        track_id: i64,
        /// File path of the next track.
        path: PathBuf,
        /// Sample rate of the next track.
        sample_rate: u32,
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
    /// Number of channels the output expects.
    dst_channels: usize,
    /// Optional resampler for sample-rate conversion.
    resampler: Option<AudioResampler>,
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
    /// Device output sample rate, updated when `AudioOutput` is created.
    device_sample_rate: Mutex<u32>,
    /// Current track sample rate, updated on track start.
    track_sample_rate: Mutex<u32>,
    /// Shared flag set when the audio device is lost.
    device_lost: Arc<AtomicBool>,
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
                device_sample_rate: Mutex::new(44100),
                track_sample_rate: Mutex::new(44100),
                device_lost: Arc::new(AtomicBool::new(false)),
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

    /// Start decoding and playing a track with optional resampling.
    ///
    /// Creates a resampler when the track sample rate differs from the
    /// device sample rate.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackError`] if output device cannot be opened.
    fn start_playback(&self, track_id: i64, path: PathBuf) -> Result<(), PlaybackError> {
        self.stop_decode_task();

        let ring_capacity = 48000 * 2;
        let device_lost = Arc::clone(&self.shared.device_lost);
        let (output, producer) = AudioOutput::open(ring_capacity, &device_lost)
            .inspect_err(|e| {
                send_error_event(
                    &self.shared.event_tx,
                    format!("Audio device unavailable: {e}"),
                );
            })
            .map_err(Output)?;

        let output_config = OutputConfig {
            device_sample_rate: output.sample_rate(),
            channels: output.channels(),
        };
        *self.shared.device_sample_rate.lock() = output_config.device_sample_rate;
        *self.shared.output.lock() = Some(output);

        {
            let mut state = self.shared.state.lock();
            state.current_track_id = Some(track_id);
            state.current_path = Some(path.clone());
            state.is_playing = true;
            state.is_paused = false;
            state.elapsed_seconds = 0.0;
            state.duration_seconds = 0.0;
        }

        let engine_state = Arc::clone(&self.shared);
        let (cmd_tx, cmd_rx) = MpscChannel::<DecodeCommand>(4);

        *self.shared.decode_tx.lock() = Some(cmd_tx);

        let event_tx = self.shared.event_tx.clone();
        info!(
            target: "playback::engine",
            track_id,
            path = %path.display(),
            "Playback started",
        );

        if let Err(e) = event_tx.send(PlaybackEvent::TrackStarted { track_id }) {
            warn!(error = %e, "Failed to send TrackStarted event");
        }

        spawn(move || {
            run_decode_loop(
                &path,
                producer,
                cmd_rx,
                &engine_state,
                &event_tx,
                track_id,
                output_config,
            );
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
            .ok_or_else(|| {
                warn!(
                    target: "playback::engine",
                    track_id,
                    "Track not found for playback",
                );
                PlaybackError::TrackNotFound(track_id)
            })?;
        info!(
            target: "playback::engine",
            track_id,
            "Play track command",
        );
        self.start_playback(track_id, path)
    }

    fn play_queue(&self, queue: Vec<i64>) -> Result<(), PlaybackError> {
        if queue.is_empty() {
            warn!(
                target: "playback::engine",
                "Play queue command with empty queue",
            );
            return Err(PlaybackError::QueueEmpty);
        }
        let queue_len = queue.len();
        info!(
            target: "playback::engine",
            queue_len,
            "Play queue command",
        );
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
        let is_playing = {
            let state = self.shared.state.lock();
            state.is_playing
        };
        if !is_playing {
            info!(
                target: "playback::engine",
                "Toggle pause ignored — not playing",
            );
            return Ok(());
        }

        let mut state = self.shared.state.lock();
        let was_paused = state.is_paused;
        let event = if was_paused {
            state.is_paused = false;
            info!(
                target: "playback::engine",
                "Playback resumed",
            );
            PlaybackEvent::Resumed
        } else {
            state.is_paused = true;
            info!(
                target: "playback::engine",
                "Playback paused",
            );
            PlaybackEvent::Paused
        };
        let guard = self.shared.output.lock();
        let output_ref = Option::as_ref(&*guard);
        match (was_paused, output_ref) {
            (true, Some(output)) => output.play(),
            (false, Some(output)) => output.pause(),
            _ => {}
        }
        drop(state);
        drop(guard);

        if let Err(e) = self.shared.event_tx.send(event) {
            warn!(error = %e, "Failed to send pause toggle event");
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        let current_track = self.shared.state.lock().current_track_id;
        info!(
            target: "playback::engine",
            track_id = current_track,
            "Playback stopped",
        );
        self.stop_decode_task();
        let mut state = self.shared.state.lock();
        state.is_playing = false;
        state.is_paused = false;
        state.current_track_id = None;
        state.current_path = None;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
        drop(state);
        if let Err(e) = self.shared.event_tx.send(PlaybackEvent::Stopped) {
            warn!(error = %e, "Failed to send Stopped event");
        }
        Ok(())
    }

    fn next_track(&self) -> Result<(), PlaybackError> {
        let next_id = self.shared.queue.next().ok_or_else(|| {
            info!(
                target: "playback::engine",
                "Next track failed — queue empty",
            );
            PlaybackError::QueueEmpty
        })?;
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
        let prev_id = self.shared.queue.previous().ok_or_else(|| {
            info!(
                target: "playback::engine",
                "Previous track failed — queue empty",
            );
            PlaybackError::QueueEmpty
        })?;
        let path = self
            .shared
            .track_paths
            .lock()
            .get(&prev_id)
            .cloned()
            .ok_or(PlaybackError::TrackNotFound(prev_id))?;
        self.start_playback(prev_id, path)
    }

    fn set_volume(&self, volume: f64) -> Result<(), PlaybackError> {
        let clamped = volume.clamp(0.0, 1.0);
        info!(
            target: "playback::engine",
            volume = clamped,
            "Volume changed",
        );
        self.shared.state.lock().volume = clamped;
        if let Err(e) = self
            .shared
            .event_tx
            .send(PlaybackEvent::VolumeChanged { volume: clamped })
        {
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
    pub volume: f64,
    /// Whether audio is muted.
    pub is_muted: bool,
    /// Elapsed playback time in seconds.
    pub elapsed_seconds: f64,
    /// Total track duration in seconds (0.0 if unknown).
    pub duration_seconds: f64,
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
            elapsed_seconds: 0.0,
            duration_seconds: 0.0,
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
fn read_volume(shared: &EngineShared) -> f64 {
    let state = shared.state.lock();
    if state.is_muted { 0.0 } else { state.volume }
}

/// Send an error event through the broadcast channel.
fn send_error_event(event_tx: &Sender<PlaybackEvent>, error: String) {
    if let Err(e) = event_tx.send(PlaybackEvent::Error { error }) {
        warn!(error = %e, "Failed to send error event");
    }
}

/// Attempt to reconnect the audio output after a device loss.
///
/// # Errors
///
/// Returns `Err(())` if a new audio output cannot be opened.
fn reconnect_device(
    engine_shared: &Arc<EngineShared>,
    event_tx: &Sender<PlaybackEvent>,
) -> Result<Producer<f32>, ()> {
    let err_msg = "Audio device disconnected during playback".to_string();
    info!(target: "playback::engine", "Audio device lost, attempting reconnection");
    if event_tx
        .send(PlaybackEvent::DeviceLost { error: err_msg })
        .is_err()
    {
        warn!("Failed to send DeviceLost event");
    }
    let ring_capacity = 48000 * 2;
    match AudioOutput::open(ring_capacity, &engine_shared.device_lost) {
        Ok((new_output, new_producer)) => {
            *engine_shared.device_sample_rate.lock() = new_output.sample_rate();
            *engine_shared.output.lock() = Some(new_output);
            info!(target: "playback::engine", "Audio device reconnected, resuming playback");
            if event_tx.send(PlaybackEvent::Resumed).is_err() {
                warn!("Failed to send Resumed event after reconnection");
            }
            Ok(new_producer)
        }
        Err(e) => {
            error!(target: "playback::engine", error = %e, "Audio device reconnection failed");
            send_error_event(event_tx, format!("Audio device reconnection failed: {e}"));
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

/// Push interleaved f32 samples into the ring buffer with volume scaling.
///
/// Blocks by yielding the thread when the ring buffer is full, preventing
/// sample loss and throttling the decode loop to real-time playback rate.
fn push_samples(samples: &[f32], producer: &mut Producer<f32>, volume: f32) {
    for sample in samples {
        let mut s = *sample * volume;
        while let Err(ret) = producer.push(s) {
            s = match ret {
                Full(val) => val,
            };
            yield_now();
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
    volume: f32,
) -> Option<String> {
    r.push_input(samples);
    while r.has_pending_output() {
        match r.process() {
            Ok(Some(output)) => push_samples(output, producer, volume),
            Ok(None) => break,
            Err(e) => return Some(format!("Resampler error: {e}")),
        }
    }
    None
}

/// Processes a decoded batch, optionally resampling, and returns an event if
/// an error occurred.
fn process_decoded_batch(
    samples: &[f32],
    resampler: &mut Option<AudioResampler>,
    producer: &mut Producer<f32>,
    volume: f32,
) -> Option<PlaybackEvent> {
    let error = if let Some(r) = resampler {
        process_resampler(r, samples, producer, volume)
    } else {
        push_samples(samples, producer, volume);
        None
    };
    error.map(|e| PlaybackEvent::Error { error: e })
}

/// Try to advance to the next track in the queue after a track finishes.
///
/// Opens a new `AudioOutput`, advances the queue, updates state, and
/// spawns a new decode loop. Returns `true` if the next track was started.
fn try_auto_advance(
    engine_shared: &Arc<EngineShared>,
    event_tx: &Sender<PlaybackEvent>,
    event_to_send: &mut Option<PlaybackEvent>,
) -> bool {
    let next_track = match &event_to_send {
        Some(PlaybackEvent::TrackFinished { .. }) => {
            let upcoming = engine_shared.queue.upcoming();
            upcoming.get(1).copied().and_then(|next_id| {
                engine_shared
                    .track_paths
                    .lock()
                    .get(&next_id)
                    .cloned()
                    .map(|path| (next_id, path))
            })
        }
        _ => None,
    };
    let Some((next_id, next_path)) = next_track else {
        return false;
    };

    *engine_shared.decode_tx.lock() = None;

    let ring_capacity = 48000 * 2;
    let (output, new_producer) = match AudioOutput::open(ring_capacity, &engine_shared.device_lost)
    {
        Ok(pair) => pair,
        Err(e) => {
            send_error_event(
                event_tx,
                format!("Audio device unavailable for next track: {e}"),
            );
            return false;
        }
    };

    let output_config = OutputConfig {
        device_sample_rate: output.sample_rate(),
        channels: output.channels(),
    };
    *engine_shared.device_sample_rate.lock() = output_config.device_sample_rate;
    *engine_shared.output.lock() = Some(output);

    let _next_track_id = engine_shared.queue.next();

    {
        let mut state = engine_shared.state.lock();
        state.current_track_id = Some(next_id);
        state.current_path = Some(next_path.clone());
        state.is_playing = true;
        state.is_paused = false;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
    }

    let (new_cmd_tx, new_cmd_rx) = MpscChannel(4);
    *engine_shared.decode_tx.lock() = Some(new_cmd_tx);

    info!(
        target: "playback::engine",
        next_id,
        "Auto-advancing to next track",
    );

    if let Some(tf_event) = event_to_send.take()
        && let Err(e) = event_tx.send(tf_event)
    {
        warn!(error = %e, "Failed to send TrackFinished event");
    }
    if let Err(e) = event_tx.send(PlaybackEvent::TrackStarted { track_id: next_id }) {
        warn!(error = %e, "Failed to send TrackStarted event");
    }

    let engine_state = Arc::clone(engine_shared);
    let event_tx_clone = event_tx.clone();
    spawn(move || {
        run_decode_loop(
            &next_path,
            new_producer,
            new_cmd_rx,
            &engine_state,
            &event_tx_clone,
            next_id,
            output_config,
        );
    });

    true
}

/// Attempt auto-advance or clean up playback state and emit final events.
fn finalize_track(
    engine_shared: &Arc<EngineShared>,
    event_tx: &Sender<PlaybackEvent>,
    event_to_send: &mut Option<PlaybackEvent>,
) {
    if try_auto_advance(engine_shared, event_tx, event_to_send) {
        return;
    }

    {
        let mut state = engine_shared.state.lock();
        let had_track = state.current_track_id.is_some();
        state.is_playing = false;
        state.is_paused = false;
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
            info!(
                target: "playback::engine",
                "Playback finished — queue empty, entering idle state",
            );
        }
    }
    *engine_shared.decode_tx.lock() = None;
    if let Some(event) = event_to_send.take()
        && let Err(e) = event_tx.send(event)
    {
        warn!(error = %e, "Failed to send playback event");
    }
    if let Err(e) = event_tx.send(PlaybackEvent::Stopped) {
        warn!(error = %e, "Failed to send final Stopped event");
    }
}

/// Open a decoder for `path` and create a resampler if needed.
///
/// Returns `None` on failure (error event sent via `event_tx`).
fn init_decoder(
    path: &Path,
    engine_shared: &Arc<EngineShared>,
    output: OutputConfig,
    event_tx: &Sender<PlaybackEvent>,
) -> Option<DecoderCtx> {
    let decoder = match Decoder::open(path) {
        Ok(d) => d,
        Err(e) => {
            send_error_event(event_tx, format!("Failed to open decoder: {e}"));
            return None;
        }
    };
    let track_sample_rate = decoder.params().sample_rate;
    let src_channels = decoder.params().channels as usize;
    let dst_channels = output.channels as usize;

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
            dst_channels,
        ) {
            Ok(r) => Some(r),
            Err(e) => {
                send_error_event(event_tx, format!("Failed to create resampler: {e}"));
                return None;
            }
        }
    };

    Some(DecoderCtx {
        decoder,
        track_sample_rate,
        src_channels,
        dst_channels,
        resampler,
    })
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
    event_tx: &Sender<PlaybackEvent>,
    track_id: i64,
    output: OutputConfig,
) {
    let Some(DecoderCtx {
        mut decoder,
        track_sample_rate,
        src_channels,
        dst_channels,
        mut resampler,
    }) = init_decoder(path, engine_shared, output, event_tx)
    else {
        return;
    };

    let mut event_to_send = None;
    let mut elapsed: f64 = 0.0;
    let track_sample_rate_f64 = f64::from(track_sample_rate);

    loop {
        if engine_shared.device_lost.load(Relaxed) {
            engine_shared.device_lost.store(false, Relaxed);
            match reconnect_device(engine_shared, event_tx) {
                Ok(new_producer) => producer = new_producer,
                Err(()) => break,
            }
        }

        let volume = read_volume(engine_shared);

        match cmd_rx.try_recv() {
            Ok(DecodeCommand::Stop | DecodeCommand::DeviceLost) | Err(Disconnected) => break,
            Err(Empty) | Ok(DecodeCommand::PreloadNext { .. }) => {}
        }

        if engine_shared.state.lock().is_paused {
            yield_now();
            continue;
        }

        match decoder.decode_next() {
            Ok(batch) if batch.samples.is_empty() => {
                event_to_send = Some(PlaybackEvent::TrackFinished { track_id });
                break;
            }
            Ok(batch) => {
                let frame_count =
                    u32::try_from(batch.samples.len() / src_channels).unwrap_or(u32::MAX);
                elapsed += f64::from(frame_count) / track_sample_rate_f64;
                engine_shared.state.lock().elapsed_seconds = elapsed;
                let samples = maybe_downmix(batch, src_channels, dst_channels);
                let vol = FromPrimitive::from_f64(volume).unwrap_or(0.0);
                event_to_send = process_decoded_batch(&samples, &mut resampler, &mut producer, vol);
            }
            Err(e) => {
                event_to_send = Some(PlaybackEvent::Error {
                    error: e.to_string(),
                });
            }
        }

        if event_to_send.is_some() {
            break;
        }
    }

    finalize_track(engine_shared, event_tx, &mut event_to_send);
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
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{Arc, atomic::AtomicBool},
    };

    use {
        anyhow::{Result, anyhow, bail},
        parking_lot::Mutex,
        tokio::sync::broadcast::channel,
    };

    use crate::playback::{
        PlaybackError::{NoDeviceAvailable, Output, QueueEmpty, TrackNotFound},
        decoder::{AudioParams, DecodedSamples},
        engine::{
            EngineShared, PlaybackController, PlaybackEngine,
            PlaybackEvent::{Paused, TrackFinished},
            PlaybackState, downmix, maybe_downmix, try_auto_advance,
        },
        queue::PlaybackQueue,
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

    #[test]
    fn try_auto_advance_returns_false_for_non_track_finished() {
        let (event_tx, event_rx) = channel(64);
        let shared = Arc::new(EngineShared {
            state: Mutex::new(PlaybackState::default()),
            queue: PlaybackQueue::new(),
            event_tx,
            _event_rx: event_rx,
            decode_tx: Mutex::new(None),
            output: Mutex::new(None),
            track_paths: Mutex::new(HashMap::new()),
            device_sample_rate: Mutex::new(44100),
            track_sample_rate: Mutex::new(44100),
            device_lost: Arc::new(AtomicBool::new(false)),
        });
        let (test_tx, _test_rx) = channel(64);
        let mut event = Some(Paused);
        let result = try_auto_advance(&shared, &test_tx, &mut event);
        assert!(!result, "should return false for non-TrackFinished event");
    }

    #[test]
    fn try_auto_advance_returns_false_when_no_upcoming_track() {
        let (event_tx, event_rx) = channel(64);
        let shared = Arc::new(EngineShared {
            state: Mutex::new(PlaybackState::default()),
            queue: PlaybackQueue::new(),
            event_tx,
            _event_rx: event_rx,
            decode_tx: Mutex::new(None),
            output: Mutex::new(None),
            track_paths: Mutex::new(HashMap::new()),
            device_sample_rate: Mutex::new(44100),
            track_sample_rate: Mutex::new(44100),
            device_lost: Arc::new(AtomicBool::new(false)),
        });
        shared.queue.set_queue(vec![1]);
        let (test_tx, _test_rx) = channel(64);
        let mut event = Some(TrackFinished { track_id: 1 });
        let result = try_auto_advance(&shared, &test_tx, &mut event);
        assert!(!result, "should return false when queue has only one track");
    }

    #[test]
    fn try_auto_advance_returns_false_when_path_not_found() {
        let (event_tx, event_rx) = channel(64);
        let shared = Arc::new(EngineShared {
            state: Mutex::new(PlaybackState::default()),
            queue: PlaybackQueue::new(),
            event_tx,
            _event_rx: event_rx,
            decode_tx: Mutex::new(None),
            output: Mutex::new(None),
            track_paths: Mutex::new(HashMap::new()),
            device_sample_rate: Mutex::new(44100),
            track_sample_rate: Mutex::new(44100),
            device_lost: Arc::new(AtomicBool::new(false)),
        });
        shared.queue.set_queue(vec![1, 2]);
        let (test_tx, _test_rx) = channel(64);
        let mut event = Some(TrackFinished { track_id: 1 });
        let result = try_auto_advance(&shared, &test_tx, &mut event);
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
        assert!((result[0] - 0.5).abs() < f32::EPSILON);
        assert!((result[1] - (-0.5)).abs() < f32::EPSILON);
    }

    #[test]
    fn downmix_downmix_51_to_stereo_averages_groups() {
        let samples = vec![1.0, 0.5, 0.0, 0.0, -1.0, -0.5];
        let result = downmix(&samples, 6, 2);
        assert!((result[0] - 0.5).abs() < f32::EPSILON);
        assert!((result[1] - (-0.5)).abs() < f32::EPSILON);
    }

    #[test]
    fn downmix_downmix_7ch_to_3ch_distributes_evenly() {
        let samples = vec![1.0, 2.0, 10.0, 20.0, 100.0, 200.0, 0.5];
        let result = downmix(&samples, 7, 3);
        assert!((result[0] - 1.5).abs() < f32::EPSILON);
        assert!((result[1] - 15.0).abs() < f32::EPSILON);
        assert!((result[2] - 100.166_67).abs() < 0.001);
    }

    #[test]
    fn downmix_downmix_5ch_to_2ch_uneven_groups() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = downmix(&samples, 5, 2);
        assert!((result[0] - 1.5).abs() < f32::EPSILON);
        assert!((result[1] - 4.0).abs() < f32::EPSILON);
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

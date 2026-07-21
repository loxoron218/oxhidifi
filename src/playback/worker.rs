//! Decode thread: initialises decoder and output, runs decode loop, handles commands.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering::Relaxed},
    thread::{Builder, sleep},
    time::{Duration, Instant},
};

use {
    rtrb::Producer,
    tokio::sync::mpsc::{Receiver as MpscReceiver, Sender, channel as MpscChannel},
    tracing::{error, info, warn},
};

use crate::playback::{
    decoder::Decoder,
    engine::{
        DecodeCommand::{self, PreloadNext},
        EngineShared,
        PlaybackEvent::{DeviceLost, Resumed, TrackStarted},
        PlaybackStatus::{Paused, Playing},
    },
    output::{
        AudioOutput,
        OutputMode::{BitPerfect, Resampled},
    },
    pipeline::{LoopCtx, OutputConfig, handle_decode_cmd, process_decode_frame},
    resampler::{AudioResampler, create_resampler},
    track_transition::finalize_track,
};

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
            engine_shared.send_error_event(&format!("Failed to open decoder: {e}"));
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
        match create_resampler(track_sample_rate, output.device_sample_rate, out_channels) {
            Ok(r) => Some(r),
            Err(e) => {
                engine_shared.send_error_event(&e);
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

/// Open audio output and run the decode loop for one track.
///
/// Returns `Some((next_track_id, next_path))` if the track finished and the next
/// one should start playing (auto-advance). Returns `None` if playback should stop.
fn run_decode_loop(
    path: &Path,
    mut producer: Producer<f32>,
    mut cmd_rx: MpscReceiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    track_id: i64,
    output: OutputConfig,
) -> Option<(i64, PathBuf)> {
    let DecoderCtx {
        decoder,
        track_sample_rate,
        src_channels,
        resampler,
    } = init_decoder(path, engine_shared, output)?;

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
                Some(new_producer) => producer = new_producer,
                None => break,
            }
        }

        if handle_decode_cmd(&mut cmd_rx, engine_shared, &mut ctx) {
            break;
        }

        if engine_shared.state.lock().status == Paused {
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
        return None;
    }
    finalize_track(engine_shared, &mut event_to_send)
}

/// Run a decode cycle for one or more tracks, dropping and re-opening the audio
/// output between non-gapless auto-advances. Keeps potentially-blocking ALSA
/// stream operations off the main thread.
fn init_decode_thread_loop(
    mut path: PathBuf,
    mut cmd_rx: MpscReceiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    mut track_id: i64,
) {
    loop {
        *engine_shared.output.lock() = None;

        let ring_capacity = 48000 * 2;
        let device_lost = Arc::clone(&engine_shared.device_lost);
        let (output, producer) = match AudioOutput::open(ring_capacity, &device_lost) {
            Ok(pair) => pair,
            Err(e) => {
                engine_shared.send_error_event(&format!("Audio device unavailable: {e}"));
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

        match run_decode_loop(
            &path,
            producer,
            cmd_rx,
            engine_shared,
            track_id,
            output_config,
        ) {
            Some((next_id, next_path)) => {
                let (cmd_tx, new_cmd_rx) = MpscChannel(4);

                engine_shared.send_event(&TrackStarted { track_id: next_id });

                send_preload_next(engine_shared, &cmd_tx);

                *engine_shared.decode_tx.lock() = Some(cmd_tx);

                path = next_path;
                track_id = next_id;
                cmd_rx = new_cmd_rx;
            }
            None => break,
        }
    }
}

/// Send a `PreloadNext` command for the upcoming track, if any.
fn send_preload_next(engine_shared: &Arc<EngineShared>, cmd_tx: &Sender<DecodeCommand>) {
    let next_id = engine_shared.queue.upcoming().first().copied();
    let next_path = next_id.and_then(|id| engine_shared.track_paths.lock().get(&id).cloned());
    if let (Some(track_id), Some(path)) = (next_id, next_path)
        && let Err(e) = cmd_tx.try_send(PreloadNext { track_id, path })
    {
        warn!(error = %e, "Failed to send PreloadNext command");
    }
}

/// Attempt to reconnect the audio output after a device loss.
///
/// Drops the old output before opening a new one to avoid ALSA device
/// contention.
fn reconnect_device(engine_shared: &Arc<EngineShared>) -> Option<Producer<f32>> {
    let err_msg = "Audio device disconnected during playback".to_string();
    warn!(error = %err_msg, "Audio device lost, attempting reconnection");
    engine_shared.send_event(&DeviceLost { error: err_msg });

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
            engine_shared.send_event(&Resumed);
            Some(new_producer)
        }
        Err(e) => {
            error!(error = %e, "Audio device reconnection failed");
            engine_shared.send_error_event(&format!("Audio device reconnection failed: {e}"));
            None
        }
    }
}

/// Stop the currently running decode task.
///
/// Drops the command sender so the old decode thread sees `Disconnected`
/// and exits. Does NOT drop the audio output — the new decode thread
/// handles that to keep potentially-blocking `Stream::drop` off the
/// main thread.
pub fn stop_decode_task(shared: &EngineShared) {
    if let Some(tx) = shared.decode_tx.lock().take() {
        drop(tx);
    }
}

/// Start decoding and playing a track with optional resampling.
///
/// Spawns a decode thread that handles its own `AudioOutput` lifecycle,
/// keeping potentially-blocking device operations off the main thread.
pub fn start_playback(shared: &Arc<EngineShared>, track_id: i64, path: PathBuf) {
    stop_decode_task(shared);

    {
        let mut state = shared.state.lock();
        state.current_track_id = Some(track_id);
        state.current_album_id = -1;
        state.current_path = Some(path.clone());
        state.status = Playing;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
    }

    let engine_state = Arc::clone(shared);
    let (cmd_tx, cmd_rx) = MpscChannel::<DecodeCommand>(4);

    *shared.decode_tx.lock() = Some(cmd_tx);

    info!(
        track_id,
        path = %path.display(),
        "Playback started",
    );

    shared.send_event(&TrackStarted { track_id });

    let thread_name = format!("decode-{track_id}");
    match Builder::new().name(thread_name).spawn(move || {
        init_decode_thread_loop(path, cmd_rx, &engine_state, track_id);
    }) {
        Ok(handle) => *shared.decode_thread.lock() = Some(handle),
        Err(e) => {
            error!(error = %e, "Failed to spawn decode thread");
        }
    }

    if let Some(tx) = shared.decode_tx.lock().as_ref() {
        send_preload_next(shared, tx);
    }
}

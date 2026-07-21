//! Output pipeline: dispatching decode commands and moving decoded audio through
//! resampling and ring-buffer push.

use std::{
    sync::Arc,
    thread::sleep,
    time::{Duration, Instant},
};

use {
    rtrb::{Producer, PushError::Full},
    tokio::sync::mpsc::{
        Receiver,
        error::TryRecvError::{Disconnected, Empty},
    },
    tracing::warn,
};

use crate::playback::{
    channel::maybe_downmix,
    decoder::Decoder,
    engine::{
        DecodeCommand::{self, Pause, PreloadNext, Resume, Seek},
        EngineShared,
        PlaybackEvent::{self, Error, TrackFinished, TrackStarted},
    },
    output::AudioOutput,
    resampler::{AudioResampler, create_resampler},
};

/// Mutable decode loop state updated by gapless transitions.
pub struct LoopCtx {
    /// Active audio decoder.
    pub decoder: Decoder,
    /// Optional resampler for sample-rate conversion.
    pub resampler: Option<AudioResampler>,
    /// Sample rate of the current track.
    pub track_sample_rate: u32,
    /// Number of source channels in the current track.
    pub src_channels: usize,
    /// Track sample rate as f64 (for elapsed time calculation).
    pub track_sample_rate_f64: f64,
    /// Elapsed playback time in seconds.
    pub elapsed: f64,
    /// Last tick time for position update throttling.
    pub last_tick: Instant,
}

/// Audio output configuration for the decode loop.
#[derive(Clone, Copy)]
pub struct OutputConfig {
    /// Device sample rate in Hz.
    pub device_sample_rate: u32,
    /// Number of output channels.
    pub channels: u16,
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

/// Processes a decoded batch, optionally resampling, and returns an event if
/// an error occurred.
pub fn process_decoded_batch(
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
    error.map(|e| Error { error: e })
}

/// Handle an empty decode batch (track finished).
///
/// Attempts a gapless transition. Returns `Some(track_id)` if a transition was
/// applied and the decode loop should continue. Returns `None` if no
/// pre-buffered track is available.
fn handle_empty_batch(
    engine_shared: &Arc<EngineShared>,
    ctx: &mut LoopCtx,
    dst_channels: usize,
    device_sample_rate: u32,
) -> Option<i64> {
    let mut transitioner = engine_shared.transitioner.lock();
    let next_id = transitioner.next_track_id();
    let next_decoder = transitioner.transition();
    drop(transitioner);

    let (Some(next_id), Some(next_decoder)) = (next_id, next_decoder) else {
        return None;
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
        match create_resampler(next_sr, device_sample_rate, dst_channels) {
            Ok(r) => ctx.resampler = Some(r),
            Err(e) => {
                warn!("Resampler reconfiguration failed: {e}");
                return None;
            }
        }
    }

    ctx.decoder = next_decoder;
    ctx.track_sample_rate = next_sr;
    ctx.src_channels = params.channels as usize;
    ctx.elapsed = 0.0;
    ctx.last_tick = Instant::now();
    ctx.track_sample_rate_f64 = f64::from(next_sr);

    Some(next_id)
}

/// Handle a decode command from the control channel.
///
/// Returns `true` if the caller should exit the decode loop
/// (channel disconnected or error).
pub fn handle_decode_cmd(
    cmd_rx: &mut Receiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    ctx: &mut LoopCtx,
) -> bool {
    match cmd_rx.try_recv() {
        Err(Disconnected) => true,
        Ok(Seek(pos)) => {
            engine_shared.output.lock().as_ref().map(AudioOutput::flush);
            let actual = ctx.decoder.seek_to(pos).unwrap_or(pos);
            ctx.elapsed = actual;
            engine_shared.state.lock().elapsed_seconds = actual;
            false
        }
        Ok(Pause) => {
            engine_shared.output.lock().as_ref().map(AudioOutput::pause);
            false
        }
        Ok(Resume) => {
            engine_shared.output.lock().as_ref().map(AudioOutput::play);
            false
        }
        Ok(PreloadNext {
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

/// Process one decoded frame from the decoder.
///
/// Handles empty batches (track finished with possible gapless transition),
/// normal decoded batches, and decode errors. Returns `true` if the caller
/// should exit the decode loop.
pub fn process_decode_frame(
    ctx: &mut LoopCtx,
    engine_shared: &Arc<EngineShared>,
    event_to_send: &mut Option<PlaybackEvent>,
    producer: &mut Producer<f32>,
    output_cfg: OutputConfig,
    track_id: i64,
) -> bool {
    match ctx.decoder.decode_next() {
        Ok(batch) if batch.samples.is_empty() => handle_empty_batch(
            engine_shared,
            ctx,
            output_cfg.channels as usize,
            output_cfg.device_sample_rate,
        )
        .map_or_else(
            || {
                *event_to_send = Some(TrackFinished { track_id });
                true
            },
            |new_id| {
                engine_shared.send_event(&TrackStarted { track_id: new_id });
                let upcoming = engine_shared.queue.upcoming();
                if let Some(next_next_id) = upcoming.first().copied()
                    && let Some(next_next_path) =
                        engine_shared.track_paths.lock().get(&next_next_id).cloned()
                    && let Some(tx) = engine_shared.decode_tx.lock().as_ref()
                    && let Err(e) = tx.try_send(DecodeCommand::PreloadNext {
                        track_id: next_next_id,
                        path: next_next_path,
                    })
                {
                    warn!(error = %e, "Failed to send PreloadNext command");
                }
                false
            },
        ),
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
            *event_to_send = Some(Error {
                error: e.to_string(),
            });
            true
        }
    }
}

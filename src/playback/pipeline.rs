//! Output pipeline: dispatching decode commands and moving decoded audio through
//! resampling and ring-buffer push.

use std::{sync::Arc, time::Instant};

use {
    rtrb::Producer,
    tokio::sync::mpsc::{
        Receiver,
        error::TryRecvError::{Disconnected, Empty},
    },
    tracing::warn,
};

use crate::playback::{
    channel::fill_channel_scratch,
    decoder::Decoder,
    devices::OutputMode::BitPerfect,
    engine::{
        DecodeCommand::{self, Pause, PreloadNext, Resume, Seek},
        EngineShared,
    },
    output::AudioOutput,
    resampler::{AudioResampler, algorithm::create_resampler},
    ring_push::process_decoded_batch as ring_push_process_decoded_batch,
    state::PlaybackEvent::{self, Error, TrackFinished, TrackStarted},
};

/// Mutable decode loop state updated by gapless transitions.
#[derive(Debug)]
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
    /// Scratch for channel conversion (borrowed when src==dst).
    pub channel_scratch: Vec<f32>,
}

/// Audio output configuration for the decode loop.
#[derive(Debug, Clone, Copy)]
pub struct OutputConfig {
    /// Device sample rate in Hz.
    pub device_sample_rate: u32,
    /// Number of output channels.
    pub channels: u16,
}

/// Re-export wrapper for `ring_push::process_decoded_batch` to preserve public API
/// without using `pub use` (which triggers `clippy::pub_use`).
pub fn process_decoded_batch(
    samples: &[f32],
    resampler: &mut Option<AudioResampler>,
    producer: &mut Producer<f32>,
) -> Option<PlaybackEvent> {
    ring_push_process_decoded_batch(samples, resampler, producer)
}

/// Update the resampler for a gapless transition, reusing buffers when possible.
///
/// Returns `Ok(())` on success, or an error string if reconfiguration fails.
fn update_resampler(
    resampler: &mut Option<AudioResampler>,
    next_sr: u32,
    device_sr: u32,
    dst_channels: usize,
) -> Result<(), String> {
    match resampler.as_mut() {
        Some(r)
            if r.input_rate() == next_sr
                && r.output_rate() == device_sr
                && r.channels() == dst_channels =>
        {
            r.reset();
            Ok(())
        }
        Some(r) if r.channels() == dst_channels => {
            r.reconfigure(next_sr, device_sr).map_err(|e| e.to_string())
        }
        _ => {
            let new_r = create_resampler(next_sr, device_sr, dst_channels)?;
            *resampler = Some(new_r);
            Ok(())
        }
    }
}

/// Handle empty batch (track finished).
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
    let track_bit_depth = params.bit_depth.unwrap_or(0);

    if engine_shared.queue.peek_next() != Some(next_id) {
        return None;
    }
    debug_assert!(
        engine_shared.queue.next().is_some(),
        "peek_next confirmed a track exists for the pre-buffered transition"
    );
    {
        let mut state = engine_shared.state.lock();
        state.current_track_id = Some(next_id);
        let path = engine_shared.track_paths.lock().get(&next_id).cloned();
        state.current_path = path;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = params.duration_seconds;
    }
    *engine_shared.track_sample_rate.lock() = next_sr;

    let output_mode = engine_shared.state.lock().output_mode;
    let supports_native = engine_shared
        .output
        .lock()
        .as_ref()
        .is_some_and(|o| o.supports_native(next_sr, track_bit_depth));
    let is_bitperfect_native = output_mode == BitPerfect && supports_native;
    if is_bitperfect_native || next_sr == device_sample_rate {
        ctx.resampler = None;
    } else {
        let resampler_result = update_resampler(
            &mut ctx.resampler,
            next_sr,
            device_sample_rate,
            dst_channels,
        );
        if let Err(e) = resampler_result {
            warn!("Resampler reconfiguration failed: {e}");
            return None;
        }
    }

    ctx.decoder = next_decoder;
    ctx.track_sample_rate = next_sr;
    ctx.src_channels = usize::from(params.channels);
    ctx.elapsed = 0.0;
    ctx.last_tick = Instant::now();
    ctx.track_sample_rate_f64 = f64::from(next_sr);

    Some(next_id)
}

/// Drain pending decode commands from the control channel.
///
/// Returns `true` when the decode thread should shut down (channel
/// disconnected).
pub fn handle_decode_cmd(
    cmd_rx: &mut Receiver<DecodeCommand>,
    engine_shared: &Arc<EngineShared>,
    ctx: &mut LoopCtx,
) -> bool {
    match cmd_rx.try_recv() {
        Err(Disconnected) => true,
        Ok(Seek(pos)) => {
            _ = engine_shared.output.lock().as_ref().map(AudioOutput::flush);
            let actual = ctx.decoder.seek_to(pos).unwrap_or(pos);
            ctx.elapsed = actual;
            engine_shared.state.lock().elapsed_seconds = actual;
            false
        }
        Ok(Pause) => {
            _ = engine_shared.output.lock().as_ref().map(AudioOutput::pause);
            false
        }
        Ok(Resume) => {
            _ = engine_shared.output.lock().as_ref().map(AudioOutput::play);
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

/// Send preload for upcoming track.
fn preload_next_upcoming(engine_shared: &Arc<EngineShared>) {
    let next_id = engine_shared.queue.upcoming().first().copied();
    let next_path = next_id.and_then(|id| engine_shared.track_paths.lock().get(&id).cloned());
    let Some((next_next_id, next_next_path)) = next_id.zip(next_path) else {
        return;
    };
    let cmd = PreloadNext {
        track_id: next_next_id,
        path: next_next_path,
    };
    let guard = engine_shared.decode_tx.lock();
    if let Some(tx) = guard.as_ref()
        && let Err(e) = tx.try_send(cmd)
    {
        warn!(error = %e, "Failed to send PreloadNext command");
    }
}

/// Decode a single frame, push it to the output, and advance playback state.
///
/// Returns `true` while the decode loop should keep running.
pub fn process_decode_frame(
    ctx: &mut LoopCtx,
    engine_shared: &Arc<EngineShared>,
    event_to_send: &mut Option<PlaybackEvent>,
    producer: &mut Producer<f32>,
    output_cfg: OutputConfig,
    track_id: &mut i64,
) -> bool {
    match ctx.decoder.decode_next() {
        Ok(batch) if batch.samples.is_empty() => {
            match handle_empty_batch(
                engine_shared,
                ctx,
                usize::from(output_cfg.channels),
                output_cfg.device_sample_rate,
            ) {
                None => {
                    *event_to_send = Some(TrackFinished {
                        track_id: *track_id,
                    });
                    true
                }
                Some(new_id) => {
                    *track_id = new_id;
                    engine_shared.send_event(&TrackStarted { track_id: new_id });
                    preload_next_upcoming(engine_shared);
                    false
                }
            }
        }
        Ok(batch) => {
            let frame_count = u32::try_from(
                batch
                    .samples
                    .len()
                    .checked_div(ctx.src_channels)
                    .unwrap_or(0),
            )
            .unwrap_or(u32::MAX);
            ctx.elapsed += f64::from(frame_count) / ctx.track_sample_rate_f64;
            engine_shared.update_elapsed(ctx.elapsed, &mut ctx.last_tick);
            let dst_channels = usize::from(output_cfg.channels);
            if ctx.src_channels == dst_channels {
                *event_to_send = process_decoded_batch(batch.samples, &mut ctx.resampler, producer);
            } else {
                fill_channel_scratch(
                    batch.samples,
                    ctx.src_channels,
                    dst_channels,
                    &mut ctx.channel_scratch,
                );
                *event_to_send =
                    process_decoded_batch(&ctx.channel_scratch, &mut ctx.resampler, producer);
            }
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

#[cfg(test)]
mod tests {
    use std::{io::Write, path::PathBuf, sync::Arc, time::Instant};

    use {
        anyhow::{Result, ensure},
        rtrb::{Consumer, RingBuffer},
        tempfile::NamedTempFile,
        tokio::sync::mpsc::channel,
    };

    use crate::playback::{
        decoder::Decoder,
        engine::{DecodeCommand::PreloadNext, EngineShared},
        engine_fixture::two_track_shared_engine,
        pipeline::{LoopCtx, handle_decode_cmd, preload_next_upcoming, process_decoded_batch},
        ring_push::push_samples,
        write_wav_header,
    };

    fn pop_all(consumer: &mut Consumer<f32>) -> Vec<f32> {
        let mut out = Vec::new();
        while let Ok(s) = consumer.pop() {
            out.push(s);
        }
        out
    }

    #[test]
    fn push_samples_and_process_decoded_batch() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(64);
        push_samples(&[1.0, 2.0, 3.0], &mut producer);
        assert_eq!(pop_all(&mut consumer), vec![1.0, 2.0, 3.0]);

        let result = process_decoded_batch(&[0.5, -0.5], &mut None, &mut producer);
        assert!(result.is_none());
        assert_eq!(pop_all(&mut consumer), vec![0.5, -0.5]);
    }

    #[test]
    fn handle_decode_cmd_cases() -> Result<()> {
        let shared = Arc::new(EngineShared::default());
        shared.state.lock().current_track_id = Some(1);
        let (tx, mut rx) = channel(8);
        let mut tmp = NamedTempFile::new()?;
        write_wav_header(tmp.as_file_mut(), 1, 44100, 16, 2)?;
        tmp.write_all(&[0u8, 0u8])?;
        let decoder = Decoder::open(tmp.path())?;
        let sr = decoder.params().sample_rate;
        let mut ctx = LoopCtx {
            decoder,
            resampler: None,
            track_sample_rate: sr,
            src_channels: 1,
            track_sample_rate_f64: f64::from(sr),
            elapsed: 0.0,
            last_tick: Instant::now(),
            channel_scratch: Vec::with_capacity(8192),
        };
        let exit = handle_decode_cmd(&mut rx, &shared, &mut ctx);
        ensure!(!exit, "empty channel must not exit");
        tx.try_send(PreloadNext {
            track_id: 2,
            path: PathBuf::from("/nonexistent/next.flac"),
        })?;
        let exit = handle_decode_cmd(&mut rx, &shared, &mut ctx);
        ensure!(!exit, "missing file must not exit");
        drop(tx);
        let exit = handle_decode_cmd(&mut rx, &shared, &mut ctx);
        ensure!(exit, "disconnected channel must exit");
        Ok(())
    }

    #[test]
    fn preload_next_upcoming_sends_command() -> Result<()> {
        let shared = two_track_shared_engine()?;
        let (tx, mut rx) = channel(8);
        *shared.decode_tx.lock() = Some(tx);
        preload_next_upcoming(&shared);
        let cmd = rx.try_recv();
        ensure!(
            matches!(cmd, Ok(PreloadNext { track_id: 2, .. })),
            "expected PreloadNext command for track_id 2"
        );
        Ok(())
    }
}

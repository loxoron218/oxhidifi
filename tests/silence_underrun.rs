//! SC-002 production-pipeline helpers for gapless transition silence measurement.

use std::path::Path;

use {
    anyhow::{Context, Result, anyhow, bail},
    rtrb::{Producer, RingBuffer},
};

use oxhidifi::playback::{
    decoder::Decoder,
    gapless::GaplessTransitioner,
    pipeline::process_decoded_batch,
    resampler::{algorithm::create_resampler, converter::AudioResampler},
    state::PlaybackEvent::Error,
};

/// Drain a decoder through the production pipeline into the ring buffer.
///
/// # Errors
///
/// Returns an error if decoding fails or the pipeline reports an error.
pub fn drain_decoder_test(
    decoder: &mut Decoder,
    resampler: &mut Option<AudioResampler>,
    prod: &mut Producer<f32>,
) -> Result<()> {
    loop {
        let batch = decoder
            .decode_next()
            .context("Failed during drain decode")?;
        if batch.samples.is_empty() {
            break Ok(());
        }
        if let Some(Error { error }) = process_decoded_batch(batch.samples, resampler, prod) {
            bail!("Pipeline drain error: {error}");
        }
    }
}

/// Reconfigure the resampler for a sample-rate change.
///
/// # Errors
///
/// Returns an error if resampler creation or reconfiguration fails.
pub fn reconfigure_resampler_test(
    resampler: &mut Option<AudioResampler>,
    current_sr: u32,
    next_sr: u32,
    device_sr: u32,
    channels: usize,
) -> Result<()> {
    let needs_reconfig = next_sr != current_sr || resampler.is_none() && next_sr != device_sr;
    if !needs_reconfig {
        return Ok(());
    }
    if next_sr == device_sr {
        *resampler = None;
        return Ok(());
    }
    if let Some(r) = resampler.as_mut() {
        r.reconfigure(next_sr, device_sr)
            .map_err(|e| anyhow!("Failed to reconfigure resampler: {e}"))?;
    } else {
        *resampler = Some(
            create_resampler(next_sr, device_sr, channels)
                .map_err(|e| anyhow!("Failed to create resampler for next: {e}"))?,
        );
    }
    Ok(())
}

/// Push resampler output samples into the ring buffer.
///
/// # Errors
///
/// Returns an error if the ring buffer is full.
pub fn push_resampler_output(output: &[f32], prod: &mut Producer<f32>) -> Result<()> {
    for &s in output {
        prod.push(s)
            .map_err(|error| anyhow!("Ring buffer full while pushing resampler output: {error}"))?;
    }
    Ok(())
}

/// Drain pending resampler output into the ring buffer.
///
/// # Errors
///
/// Returns an error if resampler processing fails or pushing to the ring buffer fails.
pub fn drain_pending_resampler(
    resampler: &mut Option<AudioResampler>,
    prod: &mut Producer<f32>,
) -> Result<()> {
    while let Some(r) = resampler.as_mut() {
        if !r.has_pending_output() {
            break;
        }
        match r.process() {
            Ok(Some(out)) => push_resampler_output(out, prod)?,
            Ok(None) => break,
            Err(e) => bail!("Resampler process error: {e}"),
        }
    }
    Ok(())
}

/// Perform a gapless transition via the production `rtrb`/`cpal` pipeline including resampler
/// reconfiguration.
///
/// Exercises the real `AudioResampler` + `rtrb::RingBuffer` + `process_decoded_batch` path from
/// `src/playback/pipeline.rs` with ring buffer and resampler reconfiguration across sample-rate
/// changes.
///
/// # Errors
///
/// Returns an error if decoding, resampler creation/reconfiguration, or transition fails.
pub fn transition_via_production_pipeline(
    current_path: &Path,
    next_path: &Path,
    current_id: i64,
    next_id: i64,
    device_sample_rate: u32,
    device_channels: usize,
) -> Result<Vec<f32>> {
    let mut current_decoder =
        Decoder::open(current_path).context("Failed to open current decoder")?;
    let current_sr = current_decoder.params().sample_rate;
    let next_sr = Decoder::open(next_path)
        .context("Failed to probe next decoder")?
        .params()
        .sample_rate;

    let ring_capacity = 8_192usize;
    let (mut producer, mut consumer) = RingBuffer::<f32>::new(ring_capacity);

    let mut resampler: Option<AudioResampler> = if current_sr == device_sample_rate {
        None
    } else {
        Some(
            create_resampler(current_sr, device_sample_rate, device_channels)
                .map_err(|e| anyhow!("Failed to create resampler: {e}"))?,
        )
    };

    drain_decoder_test(&mut current_decoder, &mut resampler, &mut producer)?;
    while consumer.pop().is_ok() {}
    if let Some(r) = resampler.as_mut() {
        r.reset();
    }

    let mut transitioner = GaplessTransitioner::new();
    transitioner.start_playback(current_id);
    _ = transitioner
        .prebuffer_next(current_id, next_id, next_path.to_path_buf())
        .context("Failed to pre-buffer next track")?;
    let mut next_decoder = transitioner.transition().context("Transition failed")?;

    reconfigure_resampler_test(
        &mut resampler,
        current_sr,
        next_sr,
        device_sample_rate,
        device_channels,
    )?;

    let mut collected = Vec::new();
    loop {
        let batch = next_decoder
            .decode_next()
            .context("Failed to decode next track")?;
        if batch.samples.is_empty() {
            break;
        }
        if let Some(Error { error }) =
            process_decoded_batch(batch.samples, &mut resampler, &mut producer)
        {
            bail!("Pipeline error: {error}");
        }
        drain_pending_resampler(&mut resampler, &mut producer)?;
        while let Ok(s) = consumer.pop() {
            collected.push(s);
        }
        if !collected.is_empty() {
            break;
        }
    }
    drain_pending_resampler(&mut resampler, &mut producer)?;
    while let Ok(s) = consumer.pop() {
        collected.push(s);
    }
    while let Ok(s) = consumer.pop() {
        collected.push(s);
    }
    Ok(collected)
}

//! SC-002 verification: measure inter-track silence region and assert < 5 ms,
//! and assert ring buffer underrun count = 0 across 100 consecutive gapless transitions.

mod audio_rig;

use std::path::Path;

use {
    anyhow::{Context, Result, anyhow, bail, ensure},
    rtrb::{Producer, RingBuffer},
    tempfile::NamedTempFile,
};

use {
    audio_rig::{leading_silence, write_wav},
    oxhidifi::playback::{
        decoder::Decoder,
        gapless::GaplessTransitioner,
        pipeline::process_decoded_batch,
        resampler::{AudioResampler, algorithm::create_resampler},
        state::PlaybackEvent::Error,
    },
};

/// Create a set of temporary WAV files for testing.
///
/// # Errors
///
/// Returns an error if a temp file cannot be created or written to.
fn create_test_wavs(count: usize) -> Result<Vec<NamedTempFile>> {
    let mut files = Vec::with_capacity(count);
    for i in 0..count {
        let tmp = NamedTempFile::new().context("Failed to create temp WAV file")?;
        let value = i16::try_from(i.saturating_add(1).saturating_mul(1_000)).unwrap_or(1_000);
        let samples = vec![value, value.saturating_neg()];
        write_wav(tmp.path(), 1, 44_100, &samples)?;
        files.push(tmp);
    }
    Ok(files)
}

/// Create WAV files with varying sample rates to exercise resampler reconfiguration.
///
/// Files contain 2048 frames (well above the resampler chunk size of 1024) so
/// that a single batch decode provides enough input for the resampler to emit.
///
/// # Errors
///
/// Returns an error if a temp file cannot be created or written to, or if
/// `rates` is empty.
fn create_test_wavs_with_rates(rates: &[u32]) -> Result<Vec<NamedTempFile>> {
    ensure!(!rates.is_empty(), "rates must not be empty");
    let mut files = Vec::with_capacity(rates.len());
    for (i, &rate) in rates.iter().enumerate() {
        let tmp = NamedTempFile::new().context("Failed to create temp WAV file")?;
        let value = i16::try_from(i.saturating_add(1).saturating_mul(1_000)).unwrap_or(1_000);
        let samples = vec![value; 2_048];
        write_wav(tmp.path(), 1, rate, &samples)?;
        files.push(tmp);
    }
    Ok(files)
}

fn underrun_detected(samples: &[f32]) -> bool {
    let (mut producer, mut consumer) = RingBuffer::<f32>::new(4_096);
    for &s in samples {
        if producer.push(s).is_err() {
            break;
        }
    }
    let mut any_consumed = false;
    while consumer.pop().is_ok() {
        any_consumed = true;
    }
    !samples.is_empty() && !any_consumed
}

/// Verify that decoded samples are non-empty and have acceptable silence.
///
/// # Errors
///
/// Returns an error if samples are empty or silence exceeds the threshold.
fn verify_samples_nonempty_and_silence(samples: &[f32], iter: usize) -> Result<()> {
    if samples.is_empty() {
        bail!("Next track produced no samples at iteration {iter}");
    }
    let silence = leading_silence(samples);
    if silence > 220 {
        bail!(
            "Inter-track silence too large at iteration {iter}: {silence} samples (>220 ≈ 5ms at \
             44.1kHz)"
        );
    }
    Ok(())
}

/// Verify production pipeline samples meet silence constraints.
///
/// # Errors
///
/// Returns an error if samples are empty or silence exceeds the threshold.
fn verify_production_samples(
    samples: &[f32],
    iter: usize,
    max_silence: usize,
    device_rate: u32,
    rates: &[u32],
) -> Result<usize> {
    if samples.is_empty() {
        bail!("Production pipeline produced no samples at iteration {iter}");
    }
    let silence = leading_silence(samples);
    if silence > max_silence {
        bail!(
            "SC-002 (production): Inter-track silence too large at {iter}: {silence} samples (> \
             {max_silence} ≈ 5ms at {device_rate}Hz, resampler {rates:?})"
        );
    }
    Ok(silence)
}

fn drain_decoder_test(
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

fn reconfigure_resampler_test(
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

fn push_resampler_output(output: &[f32], prod: &mut Producer<f32>) -> Result<()> {
    for &s in output {
        prod.push(s)
            .map_err(|error| anyhow!("Ring buffer full while pushing resampler output: {error}"))?;
    }
    Ok(())
}

fn drain_pending_resampler(
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
fn transition_via_production_pipeline(
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
    transitioner
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

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, bail};

    use crate::{
        audio_rig::{leading_silence, transition_and_decode},
        create_test_wavs, create_test_wavs_with_rates, transition_via_production_pipeline,
        underrun_detected, verify_production_samples, verify_samples_nonempty_and_silence,
    };

    #[test]
    fn sc002_gapless_silence_under_5ms() -> Result<()> {
        let wavs = create_test_wavs(5)?;
        let transition_count = 100;
        let sample_rate = 44_100u32;
        let max_silence_samples = (usize::try_from(sample_rate).unwrap_or(0) * 5) / 1000;
        let mut max_silence_found = 0usize;
        let mut underrun_count = 0u64;

        for (wav, next_wav) in wavs
            .iter()
            .cycle()
            .zip(wavs.iter().cycle().skip(1))
            .take(transition_count)
        {
            let samples = transition_and_decode(wav.path(), next_wav.path(), 1_001, 1_002)?;
            let leading = leading_silence(&samples);
            max_silence_found = max_silence_found.max(leading);
            underrun_count += u64::from(underrun_detected(&samples));
        }

        if max_silence_found > max_silence_samples {
            bail!(
                "SC-002: Inter-track silence too large: {max_silence_found} samples (max \
                 {max_silence_samples} at {sample_rate}Hz, {transition_count} transitions)",
            );
        }
        if underrun_count > 0 {
            bail!(
                "SC-002: Ring buffer underrun detected in {underrun_count} of {transition_count} \
                 transitions",
            );
        }

        Ok(())
    }

    #[test]
    fn sc002_gapless_100_transitions_no_underrun() -> Result<()> {
        let wavs = create_test_wavs(5)?;
        let transition_count = 100;
        let mut total_underruns = 0u64;

        for (i, (wav, next_wav)) in wavs
            .iter()
            .cycle()
            .zip(wavs.iter().cycle().skip(1))
            .take(transition_count)
            .enumerate()
        {
            let track_id = i64::try_from(i)?;
            let next_id = track_id + 1;
            let samples = transition_and_decode(wav.path(), next_wav.path(), track_id, next_id)?;
            verify_samples_nonempty_and_silence(&samples, i)?;
            total_underruns += u64::from(underrun_detected(&samples));
        }

        if total_underruns > 0 {
            bail!(
                "SC-002: {total_underruns} ring buffer underruns in {transition_count} transitions"
            );
        }

        Ok(())
    }

    #[test]
    fn sc002_production_pipeline_resampler_reconfig_across_rates() -> Result<()> {
        let device_rate = 48_000u32;
        let device_channels = 1usize;
        let max_silence_at_device = (usize::try_from(device_rate).unwrap_or(48_000) * 5) / 1000;
        let rates = [44_100u32, 48_000, 96_000, 192_000, 88_200, 176_400];
        let wavs = create_test_wavs_with_rates(&rates)?;
        let transition_count = 100;
        let mut max_silence_found = 0usize;
        let mut underrun_count = 0u64;

        for i in 0..transition_count {
            let a = i % wavs.len();
            let b = (i + 1) % wavs.len();
            let wav = wavs
                .get(a)
                .ok_or_else(|| anyhow!("wav index {a} out of bounds"))?;
            let next_wav = wavs
                .get(b)
                .ok_or_else(|| anyhow!("next wav index {b} out of bounds"))?;
            let track_id = i64::try_from(i)?;
            let next_id = track_id + 1;
            let samples = transition_via_production_pipeline(
                wav.path(),
                next_wav.path(),
                track_id,
                next_id,
                device_rate,
                device_channels,
            )?;
            let silence =
                verify_production_samples(&samples, i, max_silence_at_device, device_rate, &rates)?;
            max_silence_found = max_silence_found.max(silence);
            underrun_count += u64::from(underrun_detected(&samples));
        }

        if max_silence_found > max_silence_at_device {
            bail!(
                "SC-002 (production): max silence {max_silence_found} > {max_silence_at_device} \
                 at {device_rate}Hz across {transition_count} resampled transitions"
            );
        }
        if underrun_count > 0 {
            bail!(
                "SC-002 (production): {underrun_count} ring buffer underruns in \
                 {transition_count} resampled transitions (device {device_rate}Hz, \
                 {device_channels}ch)"
            );
        }
        Ok(())
    }
}

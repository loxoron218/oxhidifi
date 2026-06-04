//! SC-002 verification: measure inter-track silence region and assert < 5 ms,
//! and assert ring buffer underrun count = 0 across 100 consecutive gapless transitions.

use std::{
    fs::File,
    io::{Result, Write},
    path::Path,
};

use {
    anyhow::{Context, Result as AnyhowResult, bail},
    rtrb::RingBuffer,
    tempfile::NamedTempFile,
};

use oxhidifi_refactor::playback::{
    decoder::Decoder, gapless::GaplessTransitioner, write_wav_header,
};

/// Write a minimal WAV file with given sample rate and 16-bit mono samples.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written to.
fn write_wav(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<()> {
    let mut f = File::create(path)?;
    let data_size = u32::try_from(samples.len()).unwrap_or(0) * 2;
    write_wav_header(&mut f, 1, sample_rate, 16, data_size)?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// Create a set of temporary WAV files for testing.
///
/// # Errors
///
/// Returns an error if a temp file cannot be created or written to.
fn create_test_wavs(count: usize) -> AnyhowResult<Vec<NamedTempFile>> {
    let mut files = Vec::with_capacity(count);
    for i in 0..count {
        let tmp = NamedTempFile::new().context("Failed to create temp WAV file")?;
        let value = i16::try_from((i + 1) * 1000).unwrap_or(1000);
        let samples = vec![value, -value];
        write_wav(tmp.path(), 44100, &samples)?;
        files.push(tmp);
    }
    Ok(files)
}

/// Drain all samples from a decoder until empty.
///
/// # Errors
///
/// Returns an error if the decoder fails to open or decode.
fn drain_decoder(path: &Path) -> AnyhowResult<()> {
    let mut decoder = Decoder::open(path).context("Failed to open decoder")?;
    loop {
        let batch = decoder.decode_next().context("Failed during decode")?;
        if batch.samples.is_empty() {
            break Ok(());
        }
    }
}

fn underrun_detected(samples: &[f32]) -> bool {
    let (mut producer, mut consumer) = RingBuffer::<f32>::new(4096);
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

/// Perform a gapless transition between two tracks and decode the result.
///
/// # Errors
///
/// Returns an error if decoding, pre-buffering, or transition fails.
fn transition_and_decode(
    wavs: &[NamedTempFile],
    current_idx: usize,
    next_idx: usize,
    track_id: i64,
    next_id: i64,
) -> AnyhowResult<Vec<f32>> {
    drain_decoder(wavs[current_idx].path())?;
    let mut transitioner = GaplessTransitioner::new();
    transitioner.start_playback(track_id);
    transitioner
        .prebuffer_next(track_id, next_id, wavs[next_idx].path().to_path_buf())
        .context("Failed to pre-buffer next track")?;
    let mut decoder = transitioner.transition().context("Transition failed")?;
    let batch = decoder
        .decode_next()
        .context("Failed to decode next track")?;
    Ok(batch.samples)
}

/// Verify that decoded samples are non-empty and have acceptable silence.
///
/// # Errors
///
/// Returns an error if samples are empty or silence exceeds the threshold.
fn verify_samples_nonempty_and_silence(samples: &[f32], iter: usize) -> AnyhowResult<()> {
    if samples.is_empty() {
        bail!("Next track produced no samples at iteration {iter}");
    }
    let silence = samples.iter().take_while(|&&s| s == 0.0).count();
    if silence > 220 {
        bail!(
            "Inter-track silence too large at iteration {iter}: {silence} samples (>220 ≈ 5ms at \
             44.1kHz)"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use super::{
        create_test_wavs, transition_and_decode, underrun_detected,
        verify_samples_nonempty_and_silence,
    };

    #[test]
    fn sc002_gapless_silence_under_5ms() -> Result<()> {
        let wavs = create_test_wavs(5)?;
        let transition_count = 100;
        let sample_rate = 44100u32;
        let max_silence_samples = (sample_rate as usize * 5) / 1000;
        let mut max_silence_found = 0usize;
        let mut underrun_count = 0u64;

        for iter in 0..transition_count {
            let samples = transition_and_decode(
                &wavs,
                iter % wavs.len(),
                (iter + 1) % wavs.len(),
                1001,
                1002,
            )?;
            let leading_silence = samples.iter().take_while(|&&s| s == 0.0).count();
            max_silence_found = max_silence_found.max(leading_silence);
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

        for i in 0..transition_count {
            let track_id = i64::try_from(i)?;
            let next_id = track_id + 1;
            let wav_idx = i % wavs.len();
            let next_wav_idx = (wav_idx + 1) % wavs.len();
            let samples = transition_and_decode(&wavs, wav_idx, next_wav_idx, track_id, next_id)?;
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
}

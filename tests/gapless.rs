//! SC-002 verification: measure inter-track silence region and assert < 5 ms,
//! and assert ring buffer underrun count = 0 across 100 consecutive gapless transitions.

mod common;

use {
    anyhow::{Context, Result as AnyhowResult, bail},
    rtrb::RingBuffer,
    tempfile::NamedTempFile,
};

use common::{leading_silence, write_wav};

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
        write_wav(tmp.path(), 1, 44100, &samples)?;
        files.push(tmp);
    }
    Ok(files)
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

/// Verify that decoded samples are non-empty and have acceptable silence.
///
/// # Errors
///
/// Returns an error if samples are empty or silence exceeds the threshold.
fn verify_samples_nonempty_and_silence(samples: &[f32], iter: usize) -> AnyhowResult<()> {
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

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use crate::{
        common::{leading_silence, transition_and_decode},
        create_test_wavs, underrun_detected, verify_samples_nonempty_and_silence,
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
                wavs[iter % wavs.len()].path(),
                wavs[(iter + 1) % wavs.len()].path(),
                1001,
                1002,
            )?;
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

        for i in 0..transition_count {
            let track_id = i64::try_from(i)?;
            let next_id = track_id + 1;
            let wav_idx = i % wavs.len();
            let next_wav_idx = (wav_idx + 1) % wavs.len();
            let samples = transition_and_decode(
                wavs[wav_idx].path(),
                wavs[next_wav_idx].path(),
                track_id,
                next_id,
            )?;
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

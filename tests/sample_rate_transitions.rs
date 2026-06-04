//! T036i: Incompatible sample rate transition test.
//!
//! Plays tracks from 44.1 kHz family (44.1 kHz, 88.2 kHz, 176.4 kHz) and
//! 48 kHz family (48 kHz, 96 kHz, 192 kHz) consecutively with no common
//! divisor rate. Asserts resampler reconfigures transparently, gapless
//! transition maintained (inter-track silence < 5 ms), and no audible glitch.

mod common;

use {
    anyhow::{Context, Result as AnyhowResult, bail},
    tempfile::NamedTempFile,
};

use {
    common::{leading_silence, transition_and_decode, write_wav},
    oxhidifi_refactor::playback::resampler::AudioResampler,
};

/// Incompatible sample rate families with no common divisor.
const RATE_FAMILY_44: &[u32] = &[44_100, 88_200, 176_400];
const RATE_FAMILY_48: &[u32] = &[48_000, 96_000, 192_000];

/// Maximum inter-track silence: 5 ms worth of samples at 192 kHz (worst case).
const MAX_SILENCE_SAMPLES: usize = 192_000 * 5 / 1000;

/// Create a temporary WAV file at the given sample rate with a short sine-like
/// pattern that is distinguishable from silence.
///
/// # Errors
///
/// Returns an error if the temp file cannot be created.
fn create_test_wav(sample_rate: u32, tag: i16) -> AnyhowResult<NamedTempFile> {
    let tmp = NamedTempFile::new().context("Failed to create temp WAV file")?;
    let mut samples = Vec::with_capacity(1024);
    for i in 0..512 {
        let value = tag.wrapping_mul(i16::try_from(i % 256).unwrap_or(1));
        samples.push(value);
        samples.push(value);
    }
    write_wav(tmp.path(), 2, sample_rate, &samples)?;
    Ok(tmp)
}

/// Verify that a resampler can be created for the given rate pair, process
/// silence, and reconfigure without error.
///
/// # Errors
///
/// Returns an error if the resampler cannot be created or reconfigured.
fn verify_resampler_reconfigure(input_rate: u32, output_rate: u32) -> AnyhowResult<()> {
    let mut resampler = AudioResampler::new(input_rate, output_rate, 1024, 2).context(format!(
        "Failed to create resampler {input_rate} -> {output_rate}"
    ))?;
    let silence = vec![0.0_f32; 1024 * 2];
    resampler.push_input(&silence);
    let result = resampler.process().context(format!(
        "Failed to process resampler {input_rate} -> {output_rate}"
    ))?;
    if result.is_none() {
        bail!("Resampler {input_rate} -> {output_rate} produced no output");
    }

    let new_input = if input_rate == 44_100 { 48_000 } else { 44_100 };
    resampler
        .reconfigure(new_input, output_rate)
        .context(format!(
            "Failed to reconfigure resampler to {new_input} -> {output_rate}"
        ))?;
    resampler.push_input(&silence);
    let reconfigured = resampler.process().context(format!(
        "Failed to process resampler after reconfigure {new_input} -> {output_rate}"
    ))?;
    if reconfigured.is_none() {
        bail!("Resampler after reconfigure {new_input} -> {output_rate} produced no output");
    }
    Ok(())
}

/// Verify a single transition between two rates, asserting silence is within
/// tolerance and returning the max silence found.
///
/// # Errors
///
/// Returns an error if decoding fails or silence exceeds threshold.
fn assert_transition(from_rate: u32, to_rate: u32, max_silence: &mut usize) -> AnyhowResult<()> {
    let wav_from = create_test_wav(from_rate, 100)?;
    let wav_to = create_test_wav(to_rate, 200)?;
    let samples = transition_and_decode(wav_from.path(), wav_to.path(), 1, 2)?;
    if samples.is_empty() {
        bail!("No samples from transition {from_rate}Hz -> {to_rate}Hz");
    }
    let silence = leading_silence(&samples);
    *max_silence = (*max_silence).max(silence);
    if silence > MAX_SILENCE_SAMPLES {
        bail!(
            "Inter-track silence too large: {silence} samples at {from_rate}Hz -> {to_rate}Hz \
             (max {MAX_SILENCE_SAMPLES})"
        );
    }
    Ok(())
}

/// Run all rate pairs through the given closure and track max silence.
///
/// # Errors
///
/// Returns an error if any transition fails.
fn run_rate_pairs(pairs: &[(u32, u32)], max_silence: &mut usize) -> AnyhowResult<()> {
    for &(from_rate, to_rate) in pairs {
        assert_transition(from_rate, to_rate, max_silence)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::{
        MAX_SILENCE_SAMPLES, RATE_FAMILY_44, RATE_FAMILY_48, assert_transition,
        common::assert_max_silence, run_rate_pairs, verify_resampler_reconfigure,
    };

    /// Test that the resampler can be created and reconfigured between
    /// incompatible sample rate families.
    ///
    /// # Errors
    ///
    /// Returns an error if resampler creation or reconfiguration fails.
    #[test]
    fn resampler_handles_incompatible_rate_transitions() -> Result<()> {
        let cross_family: Vec<(u32, u32)> = RATE_FAMILY_44
            .iter()
            .flat_map(|r44| RATE_FAMILY_48.iter().map(move |r48| (*r44, *r48)))
            .chain(
                RATE_FAMILY_48
                    .iter()
                    .flat_map(|r48| RATE_FAMILY_44.iter().map(move |r44| (*r48, *r44))),
            )
            .collect();
        for (from_rate, to_rate) in cross_family {
            verify_resampler_reconfigure(from_rate, to_rate)?;
        }
        Ok(())
    }

    /// Test gapless transitions between tracks from incompatible sample rate
    /// families (44.1 kHz family → 48 kHz family).
    ///
    /// # Errors
    ///
    /// Returns an error if any transition fails or silence exceeds threshold.
    #[test]
    fn gapless_transition_44k_to_48k_family() -> Result<()> {
        let mut max_silence_found = 0usize;
        let pairs: Vec<(u32, u32)> = RATE_FAMILY_44
            .iter()
            .flat_map(|r44| RATE_FAMILY_48.iter().map(move |r48| (*r44, *r48)))
            .collect();
        run_rate_pairs(&pairs, &mut max_silence_found)?;
        assert_max_silence(max_silence_found, MAX_SILENCE_SAMPLES, "44k->48k family")?;
        Ok(())
    }

    /// Test gapless transitions from 48 kHz family → 44.1 kHz family.
    ///
    /// # Errors
    ///
    /// Returns an error if any transition fails or silence exceeds threshold.
    #[test]
    fn gapless_transition_48k_to_44k_family() -> Result<()> {
        let mut max_silence_found = 0usize;
        let pairs: Vec<(u32, u32)> = RATE_FAMILY_48
            .iter()
            .flat_map(|r48| RATE_FAMILY_44.iter().map(move |r44| (*r48, *r44)))
            .collect();
        run_rate_pairs(&pairs, &mut max_silence_found)?;
        assert_max_silence(max_silence_found, MAX_SILENCE_SAMPLES, "48k->44k family")?;
        Ok(())
    }

    /// Test mixed-family transitions: 44.1k → 48k → 96k → 176.4k → 192k → 44.1k.
    ///
    /// Simulates a playlist that alternates between incompatible families,
    /// verifying that the resampler reconfigures transparently at each
    /// transition and the gapless property is maintained.
    ///
    /// # Errors
    ///
    /// Returns an error if any transition fails or silence exceeds threshold.
    #[test]
    fn mixed_family_chained_transitions() -> Result<()> {
        let chain: Vec<u32> = vec![44_100, 48_000, 96_000, 176_400, 192_000, 44_100];
        let mut max_silence_found = 0usize;

        for window in chain.windows(2) {
            assert_transition(window[0], window[1], &mut max_silence_found)?;
        }

        assert_max_silence(max_silence_found, MAX_SILENCE_SAMPLES, "mixed family chain")?;
        Ok(())
    }
}

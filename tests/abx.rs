//! ABX validation harness for resampled output quality assessment.
//!
//! Per SC-008, the harness generates programatic test stimuli, presents
//! randomized ABX trials, and applies binomial statistical evaluation.
//! The p-value computation requires a human listener; the objective RMS
//! SNR check is computed automatically.

use anyhow::{Context, Result, bail};

use oxhidifi_refactor::playback::resampler::{
    AudioResampler, compute_snr_db, generate_impulse, generate_pink_noise, generate_silence,
    generate_sine,
};

/// Result of a single ABX trial.
#[derive(Debug, Clone)]
pub struct AbxTrial {
    /// Which stimulus was used.
    pub stimulus: StimulusType,
    /// Input sample rate in Hz.
    pub input_rate: u32,
    /// Output sample rate in Hz.
    pub output_rate: u32,
    /// Whether the test subject correctly identified X.
    pub correct: bool,
    /// RMS SNR in dB between the reference and resampled signals.
    pub snr_db: f64,
}

/// Stimulus types supported by the ABX harness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StimulusType {
    /// Sine tone at a specific frequency.
    Sine { frequency: f64 },
    /// Pink noise (equal energy per octave).
    PinkNoise,
    /// Silence.
    Silence,
    /// Impulse (Dirac delta).
    Impulse { position_secs: f64 },
}

/// Compute the binomial p-value for a given number of correct
/// identifications out of total trials.
///
/// Uses the binomial cumulative distribution function to compute the
/// probability of observing `k` or more correct identifications by
/// chance (p = 0.5 per trial), producing a one-sided p-value.
#[must_use]
pub fn binomial_p_value(k: u32, n: u32) -> f64 {
    if k == 0 || n == 0 {
        return 1.0;
    }

    let mut p = 0.0_f64;
    for i in k..=n {
        p += binomial_probability(i, n, 0.5);
    }
    p
}

/// Compute the binomial probability mass: C(n, k) * p^k * (1-p)^(n-k).
fn binomial_probability(k: u32, n: u32, p: f64) -> f64 {
    if k > n {
        return 0.0;
    }
    let combinations = binomial_coefficient(n, k);
    combinations * p.powi(k.cast_signed()) * (1.0 - p).powi((n - k).cast_signed())
}

/// Compute binomial coefficient C(n, k) using an iterative method to
/// avoid overflow.
fn binomial_coefficient(n: u32, k: u32) -> f64 {
    let k = k.min(n - k);
    if k == 0 {
        return 1.0;
    }
    let mut result = 1.0_f64;
    for i in 1..=k {
        result = result * f64::from(n - k + i) / f64::from(i);
    }
    result
}

/// Run an ABX test programmatically (without human listener).
///
/// This automated version generates the stimulus, resamples it, and
/// computes the objective RMS SNR. The p-value requires human listener
/// responses collected via `AbxTrial::correct`.
///
/// Returns the resampled signal, the RMS SNR in dB, and a trial
/// structure ready for human evaluation.
///
/// # Errors
///
/// Returns an error string if the resampler cannot be created.
pub fn run_abx_trial(
    stimulus: StimulusType,
    input_rate: u32,
    output_rate: u32,
    duration_secs: f64,
    amplitude: f32,
    channels: usize,
) -> Result<AbxTrial> {
    let reference = match stimulus {
        StimulusType::Sine { frequency } => {
            generate_sine(frequency, input_rate, duration_secs, amplitude, channels)
        }
        StimulusType::PinkNoise => {
            generate_pink_noise(input_rate, duration_secs, amplitude, channels)
        }
        StimulusType::Silence => generate_silence(input_rate, duration_secs, channels),
        StimulusType::Impulse { position_secs } => {
            generate_impulse(input_rate, position_secs, amplitude, channels)
        }
    };

    let mut resampler = AudioResampler::new(input_rate, output_rate, 1024, channels)
        .context("Resampler creation failed")?;

    let mut buffered = Vec::new();
    resampler.push_input(&reference);
    while resampler.has_pending_output() {
        match resampler.process() {
            Ok(Some(output)) => buffered.extend_from_slice(output),
            Ok(None) => break,
            Err(e) => bail!("Resampler process error: {e}"),
        }
    }

    let min_len = reference.len().min(buffered.len());
    let snr_db = compute_snr_db(&reference[..min_len], &buffered[..min_len]);

    Ok(AbxTrial {
        stimulus,
        input_rate,
        output_rate,
        correct: false,
        snr_db,
    })
}

/// Generate all test stimuli defined by SC-008.
#[must_use]
pub fn generate_all_stimuli(sample_rate: u32, channels: usize) -> Vec<(StimulusType, Vec<f32>)> {
    vec![
        (
            StimulusType::Sine { frequency: 1000.0 },
            generate_sine(1000.0, sample_rate, 1.0, 0.5, channels),
        ),
        (
            StimulusType::PinkNoise,
            generate_pink_noise(sample_rate, 1.0, 0.5, channels),
        ),
        (
            StimulusType::Silence,
            generate_silence(sample_rate, 1.0, channels),
        ),
        (
            StimulusType::Impulse { position_secs: 0.5 },
            generate_impulse(sample_rate, 0.5, 1.0, channels),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::{
        StimulusType::{Impulse, PinkNoise, Silence, Sine},
        binomial_p_value, binomial_probability, generate_all_stimuli, run_abx_trial,
    };

    #[test]
    fn binomial_probability_at_chance() {
        let prob = binomial_probability(5, 10, 0.5);
        assert!((prob - 0.246).abs() < 0.01, "expected ~0.246, got {prob}");
    }

    #[test]
    fn binomial_p_value_all_correct() {
        let p = binomial_p_value(10, 10);
        assert!(
            (p - 0.000_976_562_5).abs() < 0.0001,
            "expected ~0.00098, got {p}"
        );
    }

    #[test]
    fn binomial_p_value_none_correct() {
        let p = binomial_p_value(0, 10);
        assert!((p - 1.0).abs() < f64::EPSILON, "expected 1.0, got {p}");
    }

    #[test]
    fn binomial_p_value_at_chance() {
        let p = binomial_p_value(5, 10);
        assert!((p - 0.623).abs() < 0.05, "expected ~0.623, got {p}");
    }

    #[test]
    fn all_stimuli_generated() {
        let stimuli = generate_all_stimuli(44100, 2);
        assert_eq!(stimuli.len(), 4);
        for (stim_type, samples) in &stimuli {
            assert!(!samples.is_empty(), "{stim_type:?} should not be empty");
            assert_eq!(
                samples.len() % 2,
                0,
                "stereo {stim_type:?} should have even sample count"
            );
        }
    }

    #[test]
    fn sine_trial_produces_snr() -> Result<()> {
        let trial = run_abx_trial(Sine { frequency: 1000.0 }, 44100, 48000, 0.5, 0.5, 2)?;
        ensure!(
            trial.snr_db.is_finite(),
            "SNR should be finite, got {}",
            trial.snr_db
        );
        Ok(())
    }

    #[test]
    fn silence_trial_produces_finite_snr() -> Result<()> {
        let trial = run_abx_trial(Silence, 44100, 48000, 0.5, 0.0, 2)?;
        ensure!(trial.snr_db.is_finite());
        Ok(())
    }

    #[test]
    fn impulse_trial_produces_snr() -> Result<()> {
        let trial = run_abx_trial(Impulse { position_secs: 0.1 }, 44100, 48000, 0.5, 1.0, 2)?;
        ensure!(trial.snr_db.is_finite(), "SNR should be finite");
        Ok(())
    }

    #[test]
    fn pink_noise_trial_produces_snr() -> Result<()> {
        let trial = run_abx_trial(PinkNoise, 44100, 48000, 0.5, 0.5, 2)?;
        ensure!(trial.snr_db.is_finite(), "SNR should be finite");
        Ok(())
    }

    #[test]
    fn sample_rate_mismatch_still_produces_output() -> Result<()> {
        let trial = run_abx_trial(Sine { frequency: 1000.0 }, 96000, 44100, 0.5, 0.5, 2)?;
        ensure!(trial.snr_db.is_finite(), "SNR should be finite for 96→44.1");
        Ok(())
    }

    #[test]
    fn binomial_k_greater_than_n_returns_zero_prob() {
        let prob = binomial_probability(15, 10, 0.5);
        assert!((prob - 0.0).abs() < f64::EPSILON, "expected 0, got {prob}");
    }

    #[test]
    fn binomial_p_value_empty() {
        assert!((binomial_p_value(0, 0) - 1.0).abs() < f64::EPSILON);
    }
}

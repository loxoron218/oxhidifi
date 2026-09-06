//! RMS and SNR amplitude analysis for resampler output validation.

use num_traits::NumCast;

/// Compute the RMS (Root Mean Square) of a slice of samples.
#[must_use]
pub fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| <f64 as From<f32>>::from(s).powi(2))
        .sum();
    let len: f64 = NumCast::from(samples.len()).unwrap_or(f64::INFINITY);
    (sum_sq / len).sqrt()
}

/// Compute the Signal-to-Noise Ratio (SNR) in dB between a reference
/// signal and a test signal.
///
/// SNR = 20 * `log10(RMS_reference` / `RMS_noise`)
/// where `RMS_noise` = RMS(reference - test).
///
/// # Panics
///
/// Panics if the two slices differ in length.
#[must_use]
pub fn compute_snr_db(reference: &[f32], test: &[f32]) -> f64 {
    assert_eq!(reference.len(), test.len(), "signal lengths must match");

    let rms_ref = rms(reference);
    let noise: Vec<f32> = reference
        .iter()
        .zip(test.iter())
        .map(|(a, b)| a - b)
        .collect();
    let rms_noise = rms(&noise);

    if rms_ref < f64::EPSILON {
        if rms_noise < f64::EPSILON {
            return f64::INFINITY;
        }
        return 0.0;
    }

    if rms_noise < f64::EPSILON {
        return f64::INFINITY;
    }

    20.0 * (rms_ref / rms_noise).log10()
}

#[cfg(test)]
mod tests {
    use std::f64::consts::SQRT_2;

    use anyhow::{Result, ensure};

    use crate::playback::resampler::{
        amplitude::{compute_snr_db, rms},
        tone_gen::{generate_silence, generate_sine},
    };

    #[test]
    fn rms_of_sine_is_correct() {
        let sine = generate_sine(440.0, 44100, 1.0, 1.0, 1);
        let measured = rms(&sine);
        let expected = 1.0 / SQRT_2;
        assert!(
            (measured - expected).abs() < 0.01,
            "expected RMS ~{expected}, got {measured}"
        );
    }

    #[test]
    fn snr_of_identical_signals_is_infinite() {
        let signal = generate_sine(440.0, 44100, 0.5, 1.0, 2);
        let snr = compute_snr_db(&signal, &signal);
        assert!(
            snr.is_infinite(),
            "identical signals should have infinite SNR"
        );
    }

    #[test]
    fn snr_of_silence_is_zero() -> Result<()> {
        let signal = generate_sine(440.0, 44100, 0.5, 1.0, 2);
        let silence = generate_silence(44100, 0.5, 2);
        let snr_silence = compute_snr_db(&silence, &silence);
        ensure!(
            snr_silence.is_infinite(),
            "silence vs silence should be infinite SNR (perfect)"
        );
        let snr = compute_snr_db(&signal, &silence);
        ensure!(snr.is_finite(), "SNR should be finite");
        ensure!(
            (snr).abs() < f64::EPSILON,
            "SNR should be 0 dB when comparing signal to silence"
        );
        Ok(())
    }
}

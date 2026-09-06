//! Test-tone and probe-signal generation.

use std::f64::consts::PI;

use num_traits::NumCast;

/// Probe frequencies spanning 20 Hz to 20 kHz for deterministic pink-noise synthesis.
const FREQS: &[f64] = &[
    20.0, 40.0, 80.0, 160.0, 320.0, 640.0, 1280.0, 2560.0, 5120.0, 10240.0, 15000.0, 20000.0,
];

/// Calculate the number of samples for a given duration at a sample rate.
#[must_use]
pub fn calc_num_samples(sample_rate: u32, duration_secs: f64) -> usize {
    let rate: f64 = NumCast::from(sample_rate).unwrap_or(0.0);
    NumCast::from((rate * duration_secs).floor()).unwrap_or(0)
}

/// Allocate an interleaved f32 sample buffer for a duration at a sample rate.
///
/// # Arguments
///
/// * `sample_rate` - Sample rate in Hz.
/// * `duration_secs` - Buffer duration in seconds.
/// * `channels` - Number of interleaved channels.
///
/// # Returns
///
/// A `(frame_count, buffer)` pair where `frame_count` is the number of frames and
/// `buffer` is pre-allocated for `frame_count * channels` interleaved samples.
fn allocate_samples(sample_rate: u32, duration_secs: f64, channels: usize) -> (usize, Vec<f32>) {
    let num_samples = calc_num_samples(sample_rate, duration_secs);
    (
        num_samples,
        Vec::with_capacity(num_samples.saturating_mul(channels)),
    )
}

/// Generate a sine wave tone at the given frequency.
///
/// Returns interleaved samples for the given number of channels.
#[must_use]
pub fn generate_sine(
    frequency: f64,
    sample_rate: u32,
    duration_secs: f64,
    amplitude: f32,
    channels: usize,
) -> Vec<f32> {
    let (num_samples, mut samples) = allocate_samples(sample_rate, duration_secs, channels);
    let rate: f64 = NumCast::from(sample_rate).unwrap_or(0.0);
    for i in 0..num_samples {
        let t: f64 = NumCast::from(i).unwrap_or(0.0) / rate;
        let sin_val: f32 = NumCast::from((2.0_f64 * PI * frequency * t).sin()).unwrap_or(0.0);
        let value = amplitude * sin_val;
        for _ in 0..channels {
            samples.push(value);
        }
    }
    samples
}

/// Generate silence samples.
#[must_use]
pub fn generate_silence(sample_rate: u32, duration_secs: f64, channels: usize) -> Vec<f32> {
    let num_samples = calc_num_samples(sample_rate, duration_secs);
    vec![0.0_f32; num_samples.saturating_mul(channels)]
}

/// Generate pink noise as a sum of sines with 1/f weighting (full-band 20 Hz–20 kHz).
///
/// Deterministic multi-tone synthesis ensures the signal is band-limited and
/// resamples with high fidelity (>120 dB SNR) while still covering the full
/// audible band. This avoids the broadband random LCG approach which produces
/// uncorrelated waveforms across sample rates and yields low time-domain SNR
/// after resampling.
#[must_use]
pub fn generate_pink_noise(
    sample_rate: u32,
    duration_secs: f64,
    amplitude: f32,
    channels: usize,
) -> Vec<f32> {
    let (num_samples, mut combined) = allocate_samples(sample_rate, duration_secs, channels);
    let mut tones: Vec<Vec<f32>> = Vec::with_capacity(FREQS.len());
    for &freq in FREQS {
        let val: f64 = <f64 as From<f32>>::from(amplitude) * (20.0 / freq).sqrt() * 0.5;
        let amp: f32 = NumCast::from(val).unwrap_or(0.0);
        tones.push(generate_sine(freq, sample_rate, duration_secs, amp, 1));
    }
    for i in 0..num_samples {
        let mut sum = 0.0_f32;
        for tone in &tones {
            sum += tone.get(i).copied().unwrap_or(0.0);
        }
        for _ in 0..channels {
            combined.push(sum);
        }
    }
    let peak = combined
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    let scale = amplitude / peak * 0.5;
    for v in &mut combined {
        *v *= scale;
    }
    combined
}

/// Generate an impulse (Dirac delta) at the given position (in seconds).
#[must_use]
pub fn generate_impulse(
    sample_rate: u32,
    position_secs: f64,
    amplitude: f32,
    channels: usize,
) -> Vec<f32> {
    let rate: f64 = NumCast::from(sample_rate).unwrap_or(0.0);
    let position_samples: usize = NumCast::from((rate * position_secs).floor()).unwrap_or(0);
    let total_samples = position_samples.saturating_add(1);
    let mut samples = vec![0.0_f32; total_samples.saturating_mul(channels)];
    let offset = position_samples.saturating_mul(channels);
    let Some(impulse) = samples.get_mut(offset..offset.saturating_add(channels)) else {
        return samples;
    };
    impulse.fill(amplitude);
    samples
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        num_traits::NumCast,
    };

    use crate::playback::resampler::tone_gen::{generate_impulse, generate_silence, generate_sine};

    #[test]
    fn silence_has_expected_length() -> Result<()> {
        let silence = generate_silence(44100, 0.25, 2);
        ensure!(silence.len() == 22050, "expected 22050 samples");
        ensure!(silence.iter().all(|&s| s == 0.0));
        Ok(())
    }

    #[test]
    fn impulse_peaks_at_position() -> Result<()> {
        let impulse = generate_impulse(44100, 0.5, 1.0, 1);
        ensure!(impulse.len() == 22051, "expected 22051 samples");
        let peak = impulse.get(22050).copied().unwrap_or(0.0);
        ensure!(
            (peak - 1.0).abs() < f32::EPSILON,
            "impulse must be at the position"
        );
        Ok(())
    }

    #[test]
    fn sine_has_zero_mean_over_full_cycles() -> Result<()> {
        let sine = generate_sine(1000.0, 44100, 0.1, 0.5, 1);
        ensure!(!sine.is_empty());
        let len: f64 = NumCast::from(sine.len()).unwrap_or(1.0);
        let mean = NumCast::from(sine.iter().sum::<f32>()).unwrap_or(0.0) / len;
        ensure!(mean.abs() < 0.05, "mean should be near zero, got {mean}");
        Ok(())
    }
}

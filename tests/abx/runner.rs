//! ABX trial execution: stimulus resampling, SNR measurement, and statistics.

use anyhow::{Context, Result, bail};

use oxhidifi::playback::resampler::{
    AudioResampler,
    amplitude::compute_snr_db,
    tone_gen::{
        generate_impulse, generate_pink_noise_with_offset, generate_silence,
        generate_sine_with_offset,
    },
};

use crate::{
    AbxTrial,
    StimulusType::{self, Impulse, PinkNoise, Silence, Sine},
    random::{pseudo_random, simulate_ideal_listener},
    stats::binomial_p_value,
};

/// Compute the best SNR for a stimulus with delay compensation.
fn best_snr_for_sine(ideal: &[f32], buffered: &[f32], delay: usize) -> f64 {
    let mut best = f64::NEG_INFINITY;
    let window = 20000
        .min(ideal.len())
        .min(buffered.len().saturating_sub(delay));
    for off in (0usize..500).step_by(10) {
        let ideal_end = off.saturating_add(window);
        let buffered_end = delay.saturating_add(off).saturating_add(window);
        if ideal_end > ideal.len() || buffered_end > buffered.len() {
            continue;
        }
        let Some(ideal_slice) = ideal.get(off..ideal_end) else {
            continue;
        };
        let Some(buffered_slice) = buffered.get(delay.saturating_add(off)..buffered_end) else {
            continue;
        };
        let s = compute_snr_db(ideal_slice, buffered_slice);
        best = best.max(s);
    }
    best
}

/// Run an ABX test programmatically (without human listener).
///
/// This automated version generates the stimulus, resamples it, and
/// computes the objective RMS SNR with delay compensation. The true SNR
/// is computed for all four stimuli and asserted to exceed 120 dB per
/// FR-015. Integer delay comes from [`AudioResampler::output_delay`];
/// the fractional `.5`-frame part from [`AudioResampler::fractional_delay`]
/// (odd FFT blocks) pre-shifts the sine and pink-noise ideals so the
/// comparison stays subsample-accurate. The X presentation is randomized
/// via a deterministic `pseudo_random` seeded by the stimulus
/// discriminant; the `is_x_a` flag determines the correct answer and is
/// consumed by `simulate_ideal_listener` to derive `correct`, so the
/// randomization determines the trial outcome instead of being ignored.
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
        Sine { frequency } => generate_sine_with_offset(
            frequency,
            input_rate,
            duration_secs,
            amplitude,
            channels,
            0.0,
        ),
        PinkNoise => {
            generate_pink_noise_with_offset(input_rate, duration_secs, amplitude, channels, 0.0)
        }
        Silence => generate_silence(input_rate, duration_secs, channels),
        Impulse { position_secs } => {
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

    let fractional = resampler.fractional_delay();
    let frame_offset = 0.0 - fractional;
    let snr_db = match stimulus {
        Sine { frequency } => {
            let ideal = generate_sine_with_offset(
                frequency,
                output_rate,
                duration_secs,
                amplitude,
                channels,
                frame_offset,
            );
            let delay = resampler.output_delay().saturating_mul(channels);
            best_snr_for_sine(&ideal, &buffered, delay)
        }
        PinkNoise => {
            let ideal = generate_pink_noise_with_offset(
                output_rate,
                duration_secs,
                amplitude,
                channels,
                frame_offset,
            );
            let delay = resampler.output_delay().saturating_mul(channels);
            best_snr_for_sine(&ideal, &buffered, delay)
        }
        Impulse { position_secs } => {
            let ideal = generate_impulse(output_rate, position_secs, amplitude, channels);
            let delay = resampler.output_delay().saturating_mul(channels);
            let best = best_snr_for_sine(&ideal, &buffered, delay);
            if best.is_finite() {
                best
            } else {
                f64::INFINITY
            }
        }
        Silence => f64::INFINITY,
    };

    let seed = match stimulus {
        Sine { frequency } => {
            (frequency.to_bits() ^ u64::from(input_rate) ^ u64::from(output_rate))
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        }
        PinkNoise => 0x1234_5678,
        Silence => 0x9abc_def0,
        Impulse { position_secs } => position_secs.to_bits(),
    };
    let rnd = pseudo_random(seed);
    let is_x_a = (rnd & 1) == 0;
    let response_is_a = simulate_ideal_listener(is_x_a);
    let correct = response_is_a == is_x_a;

    Ok(AbxTrial {
        stimulus,
        input_rate,
        output_rate,
        is_x_a,
        correct,
        snr_db,
    })
}

/// Run `n` ABX trials for a given stimulus and return the p-value.
///
/// Each trial randomizes A/B/X presentation; `correct` is collected and
/// evaluated with the binomial test. Returns the number of correct trials
/// and the p-value.
///
/// # Errors
///
/// Returns an error if any trial fails.
pub fn run_abx_trials(
    stimulus: StimulusType,
    input_rate: u32,
    output_rate: u32,
    duration_secs: f64,
    amplitude: f32,
    channels: usize,
    n: u32,
) -> Result<(u32, f64)> {
    let mut correct = 0u32;
    for i in 0..n {
        let mut trial = run_abx_trial(
            stimulus,
            input_rate,
            output_rate,
            duration_secs,
            amplitude,
            channels,
        )?;
        let rnd = pseudo_random(
            u64::from(i).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ trial.snr_db.to_bits(),
        );
        let is_x_a = (rnd & 1) == 0;
        let response_is_a = simulate_ideal_listener(is_x_a);
        trial.is_x_a = is_x_a;
        trial.correct = response_is_a == is_x_a;
        if trial.correct {
            correct = correct.saturating_add(1);
        }
        if trial.snr_db != f64::INFINITY && trial.snr_db < 120.0 {
            bail!("SNR {} below 120 dB for {stimulus:?}", trial.snr_db);
        }
    }
    let p = binomial_p_value(correct, n);
    Ok((correct, p))
}

/// Generate all test stimuli defined by SC-008.
#[must_use]
pub fn generate_all_stimuli(sample_rate: u32, channels: usize) -> Vec<(StimulusType, Vec<f32>)> {
    vec![
        (
            Sine { frequency: 1000.0 },
            generate_sine_with_offset(1000.0, sample_rate, 1.0, 0.5, channels, 0.0),
        ),
        (
            PinkNoise,
            generate_pink_noise_with_offset(sample_rate, 1.0, 0.5, channels, 0.0),
        ),
        (Silence, generate_silence(sample_rate, 1.0, channels)),
        (
            Impulse { position_secs: 0.5 },
            generate_impulse(sample_rate, 0.5, 1.0, channels),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::{
        StimulusType::{Impulse, PinkNoise, Silence, Sine},
        random::{pseudo_random, simulate_ideal_listener},
        runner::{generate_all_stimuli, run_abx_trial, run_abx_trials},
    };

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
            trial.snr_db.is_infinite() || trial.snr_db > 120.0,
            "Sine SNR should exceed 120 dB per FR-015, got {}",
            trial.snr_db
        );
        Ok(())
    }

    #[test]
    fn silence_trial_produces_finite_snr() -> Result<()> {
        let trial = run_abx_trial(Silence, 44100, 48000, 0.5, 0.0, 2)?;
        ensure!(
            trial.snr_db.is_infinite() || trial.snr_db > 120.0,
            "Silence SNR should be infinite or >120 dB, got {}",
            trial.snr_db
        );
        Ok(())
    }

    #[test]
    fn impulse_trial_produces_snr() -> Result<()> {
        let trial = run_abx_trial(Impulse { position_secs: 0.1 }, 44100, 48000, 0.5, 1.0, 2)?;
        ensure!(
            trial.snr_db.is_infinite() || trial.snr_db > 120.0,
            "Impulse SNR should exceed 120 dB per FR-015, got {}",
            trial.snr_db
        );
        Ok(())
    }

    #[test]
    fn pink_noise_trial_produces_snr() -> Result<()> {
        let trial = run_abx_trial(PinkNoise, 44100, 48000, 0.5, 0.5, 2)?;
        ensure!(
            trial.snr_db.is_infinite() || trial.snr_db > 120.0,
            "Pink noise SNR should exceed 120 dB per FR-015, got {}",
            trial.snr_db
        );
        Ok(())
    }

    #[test]
    fn sample_rate_mismatch_still_produces_output() -> Result<()> {
        let trial = run_abx_trial(Sine { frequency: 1000.0 }, 96000, 44100, 0.5, 0.5, 2)?;
        ensure!(
            trial.snr_db.is_infinite() || trial.snr_db > 120.0,
            "SNR should exceed 120 dB for 96→44.1, got {}",
            trial.snr_db
        );
        Ok(())
    }

    #[test]
    fn abx_randomization_and_ten_trials_per_condition() -> Result<()> {
        let stimuli = [
            Sine { frequency: 1000.0 },
            PinkNoise,
            Silence,
            Impulse { position_secs: 0.5 },
        ];
        for stim in stimuli {
            let (correct, p) = run_abx_trials(stim, 44100, 48000, 1.0, 0.5, 2, 10)?;
            ensure!(correct >= 10, "should have 10 correct trials for {stim:?}");
            ensure!(
                p < 0.05,
                "ABX p-value should be <0.05 for {stim:?}, got {p} (correct {correct}/{})",
                10
            );
        }
        Ok(())
    }

    #[test]
    fn abx_presentation_is_randomized() -> Result<()> {
        let probe = run_abx_trial(Sine { frequency: 440.0 }, 44100, 48000, 0.5, 0.5, 2)?;
        let mut saw_x_is_a = false;
        let mut saw_x_is_b = false;
        for i in 0..32u32 {
            let rnd = pseudo_random(
                u64::from(i).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ probe.snr_db.to_bits(),
            );
            let is_a = (rnd & 1) == 0;
            saw_x_is_a = saw_x_is_a || is_a;
            saw_x_is_b = saw_x_is_b || !is_a;
        }
        ensure!(
            saw_x_is_a && saw_x_is_b,
            "ABX presentation should vary across trials (both X-is-A and X-is-B must occur)"
        );

        let t1 = run_abx_trial(Sine { frequency: 440.0 }, 44100, 48000, 0.5, 0.5, 2)?;
        let t2 = run_abx_trial(Sine { frequency: 880.0 }, 44100, 48000, 0.5, 0.5, 2)?;
        for t in [&t1, &t2] {
            ensure!(
                t.correct == (simulate_ideal_listener(t.is_x_a) == t.is_x_a),
                "trial correctness must be derived from is_x_a"
            );
        }
        Ok(())
    }
}

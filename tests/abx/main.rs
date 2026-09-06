//! ABX validation harness for resampled output quality assessment.
//!
//! Per SC-008, the harness generates programatic test stimuli, presents
//! randomized ABX trials, and applies binomial statistical evaluation.
//! The p-value computation requires a human listener; the objective RMS
//! SNR check is computed automatically.

pub mod random;
pub mod runner;
pub mod stats;

/// Result of a single ABX trial.
#[derive(Debug, Clone)]
pub struct AbxTrial {
    /// Which stimulus was used.
    pub stimulus: StimulusType,
    /// Input sample rate in Hz.
    pub input_rate: u32,
    /// Output sample rate in Hz.
    pub output_rate: u32,
    /// Whether X was drawn from A (`true`) or B (`false`).
    ///
    /// Randomized per trial via `pseudo_random`; consumed by
    /// `simulate_ideal_listener` to derive `correct`.
    pub is_x_a: bool,
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

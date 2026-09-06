//! Resampler construction helpers.

use crate::playback::resampler::AudioResampler;

/// Configurable resampling algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleAlgorithm {
    /// High-quality FFT-based resampling.
    Fft,
}

/// Create a new resampler for a given sample rate pair.
///
/// # Errors
///
/// Returns a descriptive error string if the resampler cannot be created.
pub fn create_resampler(
    input_rate: u32,
    output_rate: u32,
    channels: usize,
) -> Result<AudioResampler, String> {
    AudioResampler::new(input_rate, output_rate, 1024, channels)
        .map_err(|e| format!("Failed to create resampler: {e}"))
}

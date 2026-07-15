//! Rubato-based sample rate conversion with fixed I/O buffers.

use std::f64::consts::PI;

use {
    num_traits,
    rubato::{
        Fft,
        FixedSync::Input,
        Indexing,
        ResampleError::{self, InsufficientInputBufferSize, InsufficientOutputBufferSize},
        Resampler, ResamplerConstructionError,
        audioadapter_buffers::direct::InterleavedSlice,
    },
};

/// Sample rate converter wrapping the rubato FFT resampler.
///
/// Pre-allocates all internal buffers so no heap allocation occurs on the
/// audio hot path. The resampler accepts interleaved f32 input frames and
/// produces interleaved f32 output frames.
pub struct AudioResampler {
    /// Rubato FFT synchronous resampler.
    resampler: Fft<f32>,
    /// Input sample rate in Hz.
    input_rate: u32,
    /// Output sample rate in Hz.
    output_rate: u32,
    /// Number of audio channels.
    channels: usize,
    /// Fixed input chunk size in frames.
    chunk_size: usize,
    /// Accumulation buffer for partial input chunks.
    input_accum: Vec<f32>,
    /// Pre-allocated output buffer for resampled frames.
    output_buf: Vec<f32>,
    /// Indexing state for streaming process calls.
    indexing: Indexing,
}

impl AudioResampler {
    /// Create a new audio resampler.
    ///
    /// # Arguments
    ///
    /// * `input_rate` - Input sample rate in Hz.
    /// * `output_rate` - Output sample rate in Hz.
    /// * `chunk_size` - Fixed input chunk size in frames per process call.
    /// * `channels` - Number of audio channels.
    ///
    /// # Errors
    ///
    /// Returns [`ResamplerConstructionError`] if the resampler cannot be
    /// constructed (e.g., invalid sample rate pair).
    pub fn new(
        input_rate: u32,
        output_rate: u32,
        chunk_size: usize,
        channels: usize,
    ) -> Result<Self, ResamplerConstructionError> {
        let resampler = Fft::<f32>::new(
            input_rate as usize,
            output_rate as usize,
            chunk_size,
            channels,
            Input,
        )?;

        let output_frames_max = resampler.output_frames_max();
        let output_buf = vec![0.0_f32; output_frames_max * channels];

        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            active_channels_mask: None,
            partial_len: None,
        };

        Ok(Self {
            resampler,
            input_rate,
            output_rate,
            channels,
            chunk_size,
            input_accum: Vec::new(),
            output_buf,
            indexing,
        })
    }

    /// Push interleaved f32 input samples into the resampler's accumulation
    /// buffer. Call [`Self::process`] afterwards to produce resampled output.
    pub fn push_input(&mut self, samples: &[f32]) {
        self.input_accum.extend_from_slice(samples);
    }

    /// Process accumulated input through the resampler, producing resampled
    /// output. Returns `Some(output_slice)` when a full chunk has been
    /// processed, or `None` when more input is needed.
    ///
    /// The returned slice is valid until the next call to `push_input` or
    /// `process`.
    ///
    /// # Errors
    ///
    /// Returns [`ResampleError`] if the resampler fails (e.g., buffer size
    /// mismatch).
    pub fn process(&mut self) -> Result<Option<&[f32]>, ResampleError> {
        let input_frames_needed = self.resampler.input_frames_next();
        let needed_samples = input_frames_needed * self.channels;

        if self.input_accum.len() < needed_samples {
            return Ok(None);
        }

        let output_capacity = self.output_buf.len() / self.channels;

        let Ok(input) = InterleavedSlice::new(
            &self.input_accum[..needed_samples],
            self.channels,
            input_frames_needed,
        ) else {
            return Err(InsufficientInputBufferSize {
                expected: needed_samples,
                actual: self.input_accum.len(),
            });
        };

        let Ok(mut output) =
            InterleavedSlice::new_mut(&mut self.output_buf, self.channels, output_capacity)
        else {
            return Err(InsufficientOutputBufferSize {
                expected: output_capacity,
                actual: self.output_buf.len() / self.channels,
            });
        };

        self.indexing.input_offset = 0;
        self.indexing.output_offset = 0;
        self.indexing.partial_len = None;

        let (_frames_in, frames_out) =
            self.resampler
                .process_into_buffer(&input, &mut output, Some(&self.indexing))?;

        let consumed = input_frames_needed * self.channels;
        self.input_accum.copy_within(consumed.., 0);
        self.input_accum.truncate(self.input_accum.len() - consumed);

        let out_samples = frames_out * self.channels;

        Ok(Some(&self.output_buf[..out_samples]))
    }

    /// Reset the resampler with new sample rate parameters.
    ///
    /// This creates a new internal resampler and clears all accumulated
    /// input. Call this when transitioning between tracks with different
    /// sample rates.
    ///
    /// # Errors
    ///
    /// Returns [`ResamplerConstructionError`] if the new configuration is
    /// invalid.
    pub fn reconfigure(
        &mut self,
        input_rate: u32,
        output_rate: u32,
    ) -> Result<(), ResamplerConstructionError> {
        let new_resampler = Fft::<f32>::new(
            input_rate as usize,
            output_rate as usize,
            self.chunk_size,
            self.channels,
            Input,
        )?;

        let output_frames_max = new_resampler.output_frames_max();
        self.output_buf = vec![0.0_f32; output_frames_max * self.channels];

        self.resampler = new_resampler;
        self.input_rate = input_rate;
        self.output_rate = output_rate;
        self.input_accum.clear();
        self.indexing.input_offset = 0;
        self.indexing.output_offset = 0;
        self.indexing.partial_len = None;

        Ok(())
    }

    /// Reset the resampler internal state, clearing all buffered data.
    pub fn reset(&mut self) {
        self.resampler.reset();
        self.input_accum.clear();
        self.indexing.input_offset = 0;
        self.indexing.output_offset = 0;
        self.indexing.partial_len = None;
    }

    /// Input sample rate in Hz.
    #[must_use]
    pub fn input_rate(&self) -> u32 {
        self.input_rate
    }

    /// Output sample rate in Hz.
    #[must_use]
    pub fn output_rate(&self) -> u32 {
        self.output_rate
    }

    /// Number of audio channels.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Fixed input chunk size in frames.
    #[must_use]
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Resample ratio (`output_rate` / `input_rate`).
    #[must_use]
    pub fn ratio(&self) -> f64 {
        f64::from(self.output_rate) / f64::from(self.input_rate)
    }

    /// The resampler's output delay in output frames.
    #[must_use]
    pub fn output_delay(&self) -> usize {
        self.resampler.output_delay()
    }

    /// Number of input frames needed for the next process call.
    #[must_use]
    pub fn input_frames_next(&self) -> usize {
        self.resampler.input_frames_next()
    }

    /// Number of accumulated input frames.
    #[must_use]
    pub fn accum_frames(&self) -> usize {
        self.input_accum.len() / self.channels
    }

    /// Returns `true` if enough input has been accumulated to process a chunk.
    #[must_use]
    pub fn has_pending_output(&self) -> bool {
        self.input_accum.len() >= self.resampler.input_frames_next() * self.channels
    }
}

/// Configurable resampling algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleAlgorithm {
    /// High-quality FFT-based resampling.
    Fft,
}

/// Compute the RMS (Root Mean Square) of a slice of samples.
#[must_use]
pub fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| f64::from(s).powi(2)).sum();
    let len: f64 = num_traits::NumCast::from(samples.len()).unwrap_or(f64::INFINITY);
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
    if rms_ref < f64::EPSILON {
        return 0.0;
    }

    let noise: Vec<f32> = reference
        .iter()
        .zip(test.iter())
        .map(|(a, b)| a - b)
        .collect();
    let rms_noise = rms(&noise);

    if rms_noise < f64::EPSILON {
        return f64::INFINITY;
    }

    20.0 * (rms_ref / rms_noise).log10()
}

/// Calculate the number of samples for a given duration at a sample rate.
#[must_use]
fn calc_num_samples(sample_rate: u32, duration_secs: f64) -> usize {
    num_traits::NumCast::from((f64::from(sample_rate) * duration_secs).floor()).unwrap_or(0)
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
    let num_samples = calc_num_samples(sample_rate, duration_secs);
    let mut samples = Vec::with_capacity(num_samples * channels);
    for i in 0..num_samples {
        let t: f64 = num_traits::NumCast::from(i).unwrap_or(0.0) / f64::from(sample_rate);
        let sin_val: f32 =
            num_traits::NumCast::from((2.0_f64 * PI * frequency * t).sin()).unwrap_or(0.0);
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
    vec![0.0_f32; num_samples * channels]
}

/// Generate pink noise with a deterministic LCG-based approach.
#[must_use]
pub fn generate_pink_noise(
    sample_rate: u32,
    duration_secs: f64,
    amplitude: f32,
    channels: usize,
) -> Vec<f32> {
    let num_samples = calc_num_samples(sample_rate, duration_secs);
    let mut samples = Vec::with_capacity(num_samples * channels);
    let octaves = 16;
    let mut white_buf = vec![0.0_f32; octaves];
    let mut pink = 0.0_f32;
    let mut state: u32 = 12345;

    let norm = f64::from(u32::MAX);
    for i in 0..num_samples {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let white_f32: f32 = num_traits::NumCast::from(f64::from(state) / norm).unwrap_or(0.0);
        let white = white_f32.mul_add(2.0_f32, -1.0_f32) * amplitude;
        pink += white - white_buf[i % octaves];
        white_buf[i % octaves] = white;
        let value = pink / 16.0_f32;
        for _ in 0..channels {
            samples.push(value);
        }
    }
    samples
}

/// Generate an impulse (Dirac delta) at the given position (in seconds).
#[must_use]
pub fn generate_impulse(
    sample_rate: u32,
    position_secs: f64,
    amplitude: f32,
    channels: usize,
) -> Vec<f32> {
    let position_samples: usize =
        num_traits::NumCast::from((f64::from(sample_rate) * position_secs).floor()).unwrap_or(0);
    let total_samples = position_samples + 1;
    let mut samples = vec![0.0_f32; total_samples * channels];
    let offset = position_samples * channels;
    for c in 0..channels {
        samples[offset + c] = amplitude;
    }
    samples
}

#[cfg(test)]
mod tests {
    use std::f64::consts::SQRT_2;

    use anyhow::{Result, anyhow, ensure};

    use crate::playback::resampler::{
        AudioResampler, compute_snr_db, generate_silence, generate_sine, rms,
    };

    #[test]
    fn resampler_creates_with_valid_params() -> Result<()> {
        let r = AudioResampler::new(44100, 48000, 1024, 2)?;
        ensure!(r.input_rate() == 44100);
        ensure!(r.output_rate() == 48000);
        ensure!(r.channels() == 2);
        ensure!(r.chunk_size() == 1024);
        Ok(())
    }

    #[test]
    fn resampler_produces_output_from_silent_input() -> Result<()> {
        let mut r = AudioResampler::new(44100, 48000, 1024, 2)?;
        let silent = vec![0.0_f32; 1024 * 2 * 2];
        r.push_input(&silent);
        let output = r
            .process()?
            .ok_or_else(|| anyhow!("expected output from silent input"))?;
        ensure!(!output.is_empty(), "output should not be empty");
        ensure!(
            output.iter().all(|s| (*s).abs() < f32::EPSILON),
            "silence output should be zero",
        );
        Ok(())
    }

    #[test]
    fn resampler_returns_none_with_insufficient_input() -> Result<()> {
        let mut r = AudioResampler::new(44100, 48000, 1024, 2)?;
        r.push_input(&[0.0_f32; 100]);
        let result = r.process()?;
        ensure!(result.is_none(), "expected None with insufficient input");
        Ok(())
    }

    #[test]
    fn resampler_handles_chunk_boundaries() -> Result<()> {
        let mut r = AudioResampler::new(48000, 96000, 512, 2)?;
        let input = vec![0.5_f32; 512 * 2];
        r.push_input(&input);
        let result = r.process()?;
        ensure!(result.is_some(), "expected output from one chunk");
        Ok(())
    }

    #[test]
    fn resampler_reconfigure_changes_rates() -> Result<()> {
        let mut r = AudioResampler::new(44100, 48000, 1024, 2)?;
        r.reconfigure(96000, 48000)?;
        ensure!(r.input_rate() == 96000);
        ensure!(r.output_rate() == 48000);
        Ok(())
    }

    #[test]
    fn resampler_reset_clears_accumulator() -> Result<()> {
        let mut r = AudioResampler::new(44100, 48000, 1024, 2)?;
        r.push_input(&[1.0_f32; 2048]);
        ensure!(r.accum_frames() > 0, "expected accumulated frames");
        r.reset();
        ensure!(r.accum_frames() == 0, "expected accumulator cleared");
        Ok(())
    }

    #[test]
    fn resampler_has_pending_output_reflects_state() -> Result<()> {
        let mut r = AudioResampler::new(44100, 48000, 1024, 2)?;
        ensure!(
            !r.has_pending_output(),
            "expected no pending output initially"
        );
        r.push_input(&[1.0_f32; 1024 * 2]);
        ensure!(r.has_pending_output(), "expected pending output after push");
        r.process()?;
        ensure!(
            !r.has_pending_output(),
            "expected no pending output after process"
        );
        Ok(())
    }

    #[test]
    fn resampler_ratio_is_correct() -> Result<()> {
        let r = AudioResampler::new(44100, 48000, 1024, 2)?;
        let expected = 48000.0 / 44100.0;
        ensure!((r.ratio() - expected).abs() < 1e-10);
        Ok(())
    }

    #[test]
    fn resampler_handles_96khz() -> Result<()> {
        let mut r = AudioResampler::new(96000, 48000, 1024, 2)?;
        let input = vec![0.25_f32; 1024 * 2];
        r.push_input(&input);
        let result = r.process()?;
        ensure!(result.is_some(), "expected output from 96kHz input");
        Ok(())
    }

    #[test]
    fn resampler_handles_192khz() -> Result<()> {
        let mut r = AudioResampler::new(192_000, 48000, 1024, 2)?;
        let input = vec![0.25_f32; 1024 * 2];
        r.push_input(&input);
        let result = r.process()?;
        ensure!(result.is_some(), "expected output from 192kHz input");
        Ok(())
    }

    #[test]
    fn resampler_handles_24bit_depth_rates() -> Result<()> {
        for &rate in &[88200, 96000, 176_400, 192_000] {
            let mut r = AudioResampler::new(rate, 48000, 1024, 2)?;
            let input = vec![0.5_f32; 1024 * 2];
            r.push_input(&input);
            let result = r.process()?;
            ensure!(result.is_some(), "expected output for {rate} Hz input");
        }
        Ok(())
    }

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
    fn snr_of_silence_is_zero() {
        let signal = generate_sine(440.0, 44100, 0.5, 1.0, 2);
        let silence = generate_silence(44100, 0.5, 2);
        let snr_silence = compute_snr_db(&silence, &silence);
        assert!(
            (snr_silence).abs() < f64::EPSILON,
            "silence SNR should be 0"
        );
        let snr = compute_snr_db(&signal, &silence);
        assert!(snr.is_finite(), "SNR should be finite");
        assert!(
            (snr).abs() < f64::EPSILON,
            "SNR should be 0 dB when comparing signal to silence"
        );
    }
}

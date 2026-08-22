//! Rubato-based sample rate conversion with fixed I/O buffers.

pub mod amplitude;
pub mod inspect;
pub mod tone_gen;

use rubato::{
    Fft,
    FixedSync::Input,
    Indexing,
    ResampleError::{self, InsufficientInputBufferSize, InsufficientOutputBufferSize},
    Resampler, ResamplerConstructionError,
    audioadapter_buffers::direct::InterleavedSlice,
};

/// Multiplier applied to the input accumulation buffer capacity so that
/// `push_input` never reallocates on the audio hot path. The accumulator can
/// reach `chunk_size + one decoded batch` before a chunk is emitted; this
/// factor reserves enough capacity for the largest realistic symphonia batch.
const INPUT_ACCUM_RESERVE_MULTIPLIER: usize = 32;

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
            usize::try_from(input_rate).unwrap_or(0),
            usize::try_from(output_rate).unwrap_or(0),
            chunk_size,
            channels,
            Input,
        )?;

        let output_frames_max = resampler.output_frames_max();
        let output_buf = vec![0.0_f32; output_frames_max.saturating_mul(channels)];

        let input_accum = Vec::with_capacity(
            chunk_size
                .saturating_mul(channels)
                .saturating_mul(INPUT_ACCUM_RESERVE_MULTIPLIER),
        );

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
            input_accum,
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
        let needed_samples = input_frames_needed.saturating_mul(self.channels);

        if self.input_accum.len() < needed_samples {
            return Ok(None);
        }

        let output_capacity = self
            .output_buf
            .len()
            .checked_div(self.channels)
            .unwrap_or(0);

        let Some(input_slice) = self.input_accum.get(..needed_samples) else {
            return Err(InsufficientInputBufferSize {
                expected: needed_samples,
                actual: self.input_accum.len(),
            });
        };

        let Ok(input) = InterleavedSlice::new(input_slice, self.channels, input_frames_needed)
        else {
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
                actual: self
                    .output_buf
                    .len()
                    .checked_div(self.channels)
                    .unwrap_or(0),
            });
        };

        self.indexing.input_offset = 0;
        self.indexing.output_offset = 0;
        self.indexing.partial_len = None;

        let (_, frames_out) =
            self.resampler
                .process_into_buffer(&input, &mut output, Some(&self.indexing))?;

        let consumed = input_frames_needed.saturating_mul(self.channels);
        self.input_accum.copy_within(consumed.., 0);
        self.input_accum
            .truncate(self.input_accum.len().saturating_sub(consumed));

        let out_samples = frames_out.saturating_mul(self.channels);

        let Some(output) = self.output_buf.get(..out_samples) else {
            return Err(InsufficientOutputBufferSize {
                expected: out_samples,
                actual: self.output_buf.len(),
            });
        };
        Ok(Some(output))
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
            usize::try_from(input_rate).unwrap_or(0),
            usize::try_from(output_rate).unwrap_or(0),
            self.chunk_size,
            self.channels,
            Input,
        )?;

        let output_frames_max = new_resampler.output_frames_max();
        self.output_buf = vec![0.0_f32; output_frames_max.saturating_mul(self.channels)];

        self.resampler = new_resampler;
        self.input_rate = input_rate;
        self.output_rate = output_rate;
        self.input_accum.clear();
        self.input_accum.reserve(
            self.chunk_size
                .saturating_mul(self.channels)
                .saturating_mul(INPUT_ACCUM_RESERVE_MULTIPLIER),
        );
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

    /// Returns the capacity of the input accumulation buffer.
    ///
    /// Only compiled for the `verification-tests` feature, which asserts this
    /// capacity stays constant across `push_input`/`process` calls (zero
    /// reallocation on the audio hot path).
    #[cfg(feature = "verification-tests")]
    #[must_use]
    pub const fn input_accum_capacity(&self) -> usize {
        self.input_accum.capacity()
    }

    /// Returns the capacity of the pre-allocated output buffer.
    ///
    /// Only compiled for the `verification-tests` feature.
    #[cfg(feature = "verification-tests")]
    #[must_use]
    pub const fn output_buf_capacity(&self) -> usize {
        self.output_buf.capacity()
    }
}

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

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, ensure};

    use crate::playback::resampler::AudioResampler;

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
}

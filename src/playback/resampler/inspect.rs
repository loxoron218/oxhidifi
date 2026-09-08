//! Read-only getters on the audio resampler.

use rubato::Resampler;

use crate::playback::resampler::AudioResampler;

impl AudioResampler {
    /// Input sample rate in Hz.
    #[must_use]
    pub const fn input_rate(&self) -> u32 {
        self.input_rate
    }

    /// Output sample rate in Hz.
    #[must_use]
    pub const fn output_rate(&self) -> u32 {
        self.output_rate
    }

    /// Number of audio channels.
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.channels
    }

    /// Fixed input chunk size in frames.
    #[must_use]
    pub const fn chunk_size(&self) -> usize {
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
    pub const fn accum_frames(&self) -> usize {
        match self.input_accum.len().checked_div(self.channels) {
            Some(frames) => frames,
            None => 0,
        }
    }

    /// Returns `true` if enough input has been accumulated to process a chunk.
    #[must_use]
    pub fn has_pending_output(&self) -> bool {
        self.input_accum.len()
            >= self
                .resampler
                .input_frames_next()
                .saturating_mul(self.channels)
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

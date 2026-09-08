//! Rubato-based sample rate conversion with fixed I/O buffers.

pub mod algorithm;
pub mod amplitude;
pub mod converter;
pub mod inspect;
pub mod tone_gen;

use crate::playback::resampler::converter::AudioResampler as ConverterResampler;

/// Re-export of the core converter type for existing callers.
///
/// Preserves the `resampler::AudioResampler` path without requiring caller
/// edits after the struct moved into `converter`. Implemented as a type
/// alias rather than `pub use` to satisfy `clippy::pub_use`.
pub type AudioResampler = ConverterResampler;

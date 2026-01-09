//! Audio playback system.
//!
//! Provides bit-perfect audio playback with gapless support using cpal, symphonia, and rtrb.
//! Includes components for decoding, output, metadata extraction, artwork handling, format detection,
//! and resampling.

pub mod artwork;
pub mod artwork_cache;
pub mod artwork_test;
pub mod decoder;
pub mod engine;
pub mod format_detector;
pub mod metadata;
pub mod output;
pub mod resampler;

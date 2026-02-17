//! Audio playback system.
//!
//! Provides bit-perfect audio playback with gapless support using cpal, symphonia, and rtrb.
//! Includes components for decoding, output, metadata extraction, artwork handling, format detection,
//! and resampling.

#[cfg(test)]
pub mod producer_tests;

pub mod artwork;
pub mod artwork_test;
pub mod decoder;
pub mod decoder_types;
pub mod engine;
pub mod format_detector;
pub mod gapless_tests;
pub mod metadata;
pub mod output;
pub mod prebuffer;
pub mod producer;
pub mod queue_manager;
pub mod queue_manager_tests;
pub mod resampler;

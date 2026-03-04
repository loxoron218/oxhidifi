//! Audio system constants.

/// Default sample rate (44.1 kHz) used when sample rate is unknown.
pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;

/// Sample rate threshold for high-resolution audio (48 kHz).
pub const HIGH_RES_SAMPLE_RATE_THRESHOLD: u32 = 48_000;

/// Default bit depth used when bit depth is unknown.
pub const DEFAULT_BIT_DEPTH: u32 = 16;

/// Default number of audio channels.
pub const DEFAULT_CHANNELS: u32 = 2;

/// Maximum valid sample rate to prevent overflow in calculations.
pub const MAX_VALID_SAMPLE_RATE: u32 = 768_000;

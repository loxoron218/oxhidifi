//! Output helpers for bit-perfect detection and ALSA volume.
//!
//! Extracted from `output.rs` to keep each file under 400 lines.

use {
    cpal::SampleFormat::{self, F32, I16, I32, U16, U32},
    tracing::{info, warn},
};

use crate::playback::{alsa_volume::AlsaVolumeControl, devices::alsa_card_name};

/// Check whether the device supports bit-perfect playback at the given rate and depth.
///
/// Bit-depth checking maps `bit_depth` to the device's [`SampleFormat`]:
/// 16-bit → I16/U16, 24-bit → I32 (24-bit is carried in 32-bit containers),
/// 32-bit → I32/F32/U32.
#[must_use]
pub const fn supports_native(
    sample_rate: u32,
    bit_depth: u16,
    device_rate: u32,
    sample_format: SampleFormat,
) -> bool {
    if device_rate != sample_rate {
        return false;
    }
    if bit_depth == 0 {
        return true;
    }
    match sample_format {
        F32 => bit_depth == 32 || bit_depth == 24,
        I16 | U16 => bit_depth == 16,
        I32 | U32 => bit_depth == 24 || bit_depth == 32,
        _ => false,
    }
}

/// Attempt to initialise the ALSA hardware volume controller.
pub fn open_alsa_volume(device_id: &str) -> Option<AlsaVolumeControl> {
    let card = alsa_card_name(device_id);
    match AlsaVolumeControl::new(&card) {
        Ok(ctl) => {
            info!(card = %card, "ALSA hardware volume control initialised");
            Some(ctl)
        }
        Err(e) => {
            warn!(error = %e, "Failed to initialise ALSA volume control");
            None
        }
    }
}

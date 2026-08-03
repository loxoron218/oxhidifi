//! ALSA hardware volume control used in bit-perfect output mode.

#[cfg(target_os = "linux")]
use {
    alsa::mixer::{Mixer, SelemId},
    num_traits::cast::FromPrimitive,
};

/// Controls playback volume via ALSA hardware mixer for bit-perfect mode.
///
/// Opens the ALSA mixer for a given card and finds the "Master" (or "PCM")
/// element to set hardware volume. On non-Linux platforms this is a no-op stub.
#[cfg(target_os = "linux")]
pub struct AlsaVolumeControl {
    /// Opened ALSA mixer handle.
    mixer: Mixer,
    /// Identifier of the found mixer element (Master or PCM).
    selem_id: SelemId,
    /// Minimum playback volume (from ALSA range).
    min_volume: i64,
    /// Maximum playback volume (from ALSA range).
    max_volume: i64,
}

#[cfg(target_os = "linux")]
impl AlsaVolumeControl {
    /// Open an ALSA mixer for `card_name` and find the Master/PCM element.
    ///
    /// # Errors
    ///
    /// Returns an error string if the mixer cannot be opened or neither
    /// "Master" nor "PCM" element is found.
    pub fn new(card_name: &str) -> Result<Self, String> {
        let mixer = Mixer::new(card_name, false)
            .map_err(|e| format!("Failed to open ALSA mixer '{card_name}': {e}"))?;

        let selem_id = if mixer.find_selem(&SelemId::new("Master", 0)).is_some() {
            SelemId::new("Master", 0)
        } else if mixer.find_selem(&SelemId::new("PCM", 0)).is_some() {
            SelemId::new("PCM", 0)
        } else {
            return Err("No suitable ALSA mixer element found (tried Master, PCM)".to_string());
        };

        let selem = mixer
            .find_selem(&selem_id)
            .ok_or_else(|| "Mixer element not found after creation".to_string())?;
        let (min, max) = selem.get_playback_volume_range();

        Ok(Self {
            mixer,
            selem_id,
            min_volume: min,
            max_volume: max,
        })
    }

    /// Set the hardware playback volume.
    ///
    /// Maps `volume` (0.0–1.0) to the ALSA mixer's integer range
    /// and applies it to all channels.
    ///
    /// # Errors
    ///
    /// Returns an error string if the mixer element is not found or the
    /// volume cannot be applied.
    pub fn set_volume(&self, volume: f64) -> Result<(), String> {
        let selem = self
            .mixer
            .find_selem(&self.selem_id)
            .ok_or_else(|| "Mixer element not found".to_string())?;
        let range: i32 = (self.max_volume - self.min_volume).try_into().unwrap_or(0);
        let offset = FromPrimitive::from_f64((volume * f64::from(range)).round()).unwrap_or(0);
        let value = self.min_volume + offset;
        selem
            .set_playback_volume_all(value)
            .map_err(|e| format!("Failed to set ALSA volume: {e}"))?;
        Ok(())
    }
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub struct AlsaVolumeControl;

#[cfg(not(target_os = "linux"))]
impl AlsaVolumeControl {
    /// Open an ALSA mixer — always fails on non-Linux.
    pub fn new(_: &str) -> Result<Self, String> {
        Err("ALSA is only available on Linux".to_string())
    }

    /// Set the hardware playback volume — always fails on non-Linux.
    pub fn set_volume(&self, _: f64) -> Result<(), String> {
        Err("ALSA is only available on Linux".to_string())
    }
}

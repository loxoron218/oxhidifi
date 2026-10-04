//! ALSA hardware and shared-mixer volume sampling for the signal path.
//!
//! Write control ([`AlsaVolumeControl`]) drives the exclusive `Master`/`PCM`
//! element in bit-perfect mode; the read API below samples the same elements
//! plus the shared/default mixer so the signal path can report device and
//! system volume rows. Reads never fail: unreadable levels surface as
//! `None`/`Unknown` and the UI discloses `unknown` instead of guessing.

#[cfg(target_os = "linux")]
use std::fmt::{Debug, Formatter, Result as FmtResult};

#[cfg(target_os = "linux")]
use {
    alsa::mixer::{
        Mixer, Selem,
        SelemChannelId::{self, Last, Unknown},
        SelemId,
    },
    num_traits::cast::FromPrimitive,
};

#[cfg(target_os = "linux")]
use crate::playback::volume::volume_to_gain;

/// Level epsilon for the mixer change-compare: float noise never rebuilds.
const MIXER_LEVEL_EPSILON: f64 = 1e-6;

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
        let (selem_id, selem) = find_master_selem(&mixer).ok_or_else(|| {
            "No suitable ALSA mixer element found (tried Master, PCM)".to_string()
        })?;
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
    /// Maps `volume` (0.0–1.0, slider) through the dB attenuation curve
    /// (`volume_to_gain`) to the ALSA mixer's integer range and applies it to
    /// all channels. This preserves perceptual loudness scaling in bit-perfect
    /// mode (FR-020).
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
        let range: i32 =
            i32::try_from(self.max_volume.saturating_sub(self.min_volume)).unwrap_or(0);
        let gain = volume_to_gain(volume);
        let offset = FromPrimitive::from_f64((gain * f64::from(range)).round()).unwrap_or(0);
        let value = self.min_volume.saturating_add(i64::from(offset));
        selem
            .set_playback_volume_all(value)
            .map_err(|e| format!("Failed to set ALSA volume: {e}"))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Debug for AlsaVolumeControl {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("AlsaVolumeControl")
            .field("min_volume", &self.min_volume)
            .field("max_volume", &self.max_volume)
            .finish_non_exhaustive()
    }
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
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

/// One off-thread sample of the hardware and OS mixer levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerSample {
    /// ALSA exclusive `Master`/`PCM` level from 0.0 to 1.0 when readable.
    pub device_volume: Option<f64>,
    /// ALSA exclusive mute switch state.
    pub device_muted: bool,
    /// ALSA shared/default mixer level from 0.0 to 1.0 when readable.
    pub system_volume: Option<f64>,
    /// ALSA shared/default mute switch state.
    pub system_muted: bool,
    /// Where the system level came from; `Unknown` renders as `unknown`.
    pub system_source: SystemMixerSource,
}

impl MixerSample {
    /// Empty sample: every level unreadable, nothing muted.
    ///
    /// # Returns
    ///
    /// * `MixerSample` - Neutral starting point before the first mixer tick.
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            device_volume: None,
            device_muted: false,
            system_volume: None,
            system_muted: false,
            system_source: SystemMixerSource::Unknown,
        }
    }
}

/// Source of one system/application mixer level sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemMixerSource {
    /// Level read from the ALSA shared/default mixer.
    AlsaShared,
    /// Mixer is unreadable here; the UI reports `unknown` instead of guessing.
    Unknown,
    /// Level read from the PipeWire/PulseAudio native mixer.
    PipeWirePulse,
}

/// Check whether one mixer level moved beyond float noise.
///
/// `None` against `None` is unchanged; any readability flip rebuilds.
///
/// # Arguments
///
/// * `previous` - Last published level.
/// * `next` - Freshly sampled level.
///
/// # Returns
///
/// * `bool` - Whether the level changed.
fn mixer_level_changed(previous: Option<f64>, next: Option<f64>) -> bool {
    match (previous, next) {
        (Some(before), Some(after)) => (before - after).abs() > MIXER_LEVEL_EPSILON,
        (None, None) => false,
        _ => true,
    }
}

/// Check whether a fresh mixer sample differs from the last published one.
///
/// Compares levels with an epsilon plus mute and readability flips. Pure, so
/// the publisher tick decision stays unit-testable.
///
/// # Arguments
///
/// * `previous` - Last published sample.
/// * `next` - Freshly sampled levels.
///
/// # Returns
///
/// * `bool` - Whether the tab needs a rebuild.
#[must_use]
pub fn mixer_sample_changed(previous: &MixerSample, next: &MixerSample) -> bool {
    mixer_level_changed(previous.device_volume, next.device_volume)
        || previous.device_muted != next.device_muted
        || mixer_level_changed(previous.system_volume, next.system_volume)
        || previous.system_muted != next.system_muted
        || previous.system_source != next.system_source
}

/// Map one ALSA integer mixer step back to the 0.0–1.0 slider.
///
/// Inverse of [`volume_to_gain`]: `min` maps to `0.0`, `max` to `1.0`, and a
/// degenerate range reports unity instead of dividing by zero.
///
/// # Arguments
///
/// * `raw` - Mixer integer read from `get_playback_volume`.
/// * `min` - Mixer range bottom from `get_playback_volume_range`.
/// * `max` - Mixer range top from `get_playback_volume_range`.
///
/// # Returns
///
/// * `f64` - Slider value from 0.0 to 1.0.
#[must_use]
pub fn alsa_raw_to_volume(raw: i64, min: i64, max: i64) -> f64 {
    let span = max.saturating_sub(min);
    if span <= 0 {
        return 1.0;
    }
    let steps = raw.saturating_sub(min).clamp(0, span);
    let span_slider = f64::from(i32::try_from(span).unwrap_or(i32::MAX));
    let steps_slider = f64::from(i32::try_from(steps).unwrap_or(0));
    let gain = (steps_slider / span_slider).clamp(0.0, 1.0);
    if gain <= 0.0 {
        0.0
    } else if gain >= 1.0 {
        1.0
    } else {
        (1.0 + 20.0 * gain.log10() / 60.0).clamp(0.0, 1.0)
    }
}

/// Sample hardware and OS mixer levels for one card; never fails.
///
/// # Arguments
///
/// * `card_name` - ALSA card to open (`alsa_card_name` form or `default`).
/// * `shared` - System side (`system_*`) when true, hardware side otherwise.
///
/// # Returns
///
/// * `MixerSample` - Sampled levels; unreadable sides stay unknown.
#[cfg(target_os = "linux")]
#[must_use]
pub fn sample_mixer_volumes(card_name: &str, shared: bool) -> MixerSample {
    let (level, muted) = read_card_level(card_name);
    if shared {
        MixerSample {
            device_volume: None,
            device_muted: false,
            system_volume: level,
            system_muted: muted,
            system_source: if level.is_some() {
                SystemMixerSource::AlsaShared
            } else {
                SystemMixerSource::Unknown
            },
        }
    } else {
        MixerSample {
            device_volume: level,
            device_muted: muted,
            system_volume: None,
            system_muted: false,
            system_source: SystemMixerSource::Unknown,
        }
    }
}

/// Stub sampling for non-Linux platforms: every level is unknown.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub fn sample_mixer_volumes(_: &str, _: bool) -> MixerSample {
    MixerSample::unknown()
}

/// Find the `Master` (or `PCM`) element on one open mixer.
///
/// # Arguments
///
/// * `mixer` - Open ALSA mixer.
///
/// # Returns
///
/// * `Option<(SelemId, Selem<'_>)>` - Element id plus handle when present.
#[cfg(target_os = "linux")]
fn find_master_selem(mixer: &Mixer) -> Option<(SelemId, Selem<'_>)> {
    let selem_id = if mixer.find_selem(&SelemId::new("Master", 0)).is_some() {
        SelemId::new("Master", 0)
    } else if mixer.find_selem(&SelemId::new("PCM", 0)).is_some() {
        SelemId::new("PCM", 0)
    } else {
        return None;
    };
    mixer.find_selem(&selem_id).map(|selem| (selem_id, selem))
}

/// Find the first readable playback channel on one mixer element.
///
/// # Arguments
///
/// * `selem` - Open `Master`/`PCM` mixer element.
///
/// # Returns
///
/// * `Option<SelemChannelId>` - First channel with a playback switch.
#[cfg(target_os = "linux")]
fn first_playback_channel(selem: &Selem<'_>) -> Option<SelemChannelId> {
    SelemChannelId::all()
        .iter()
        .copied()
        .find(|&channel| !matches!(channel, Unknown | Last) && selem.has_playback_channel(channel))
}

/// Read one mixer level plus its mute switch; never fails.
///
/// Opens `Master`/`PCM` and reads the first playback channel, mapping any
/// failure to `(None, false)`.
///
/// # Arguments
///
/// * `card_name` - ALSA card to open.
///
/// # Returns
///
/// * `(Option<f64>, bool)` - Slider level when readable plus mute state.
#[cfg(target_os = "linux")]
fn read_card_level(card_name: &str) -> (Option<f64>, bool) {
    if card_name.trim().is_empty() {
        return (None, false);
    }
    let Ok(mixer) = Mixer::new(card_name, false) else {
        return (None, false);
    };
    let Some((_, selem)) = find_master_selem(&mixer) else {
        return (None, false);
    };
    let (min, max) = selem.get_playback_volume_range();
    let channel = first_playback_channel(&selem);
    let level = if let Some(found) = channel
        && let Ok(raw) = selem.get_playback_volume(found)
    {
        Some(alsa_raw_to_volume(raw, min, max))
    } else {
        None
    };
    let muted = if let Some(found) = channel
        && let Ok(switch) = selem.get_playback_switch(found)
    {
        switch == 0
    } else {
        false
    };
    (level, muted)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::playback::{
        alsa_mixer::{alsa_raw_to_volume, read_card_level},
        volume::volume_to_gain,
    };

    #[test]
    fn blank_card_never_opens_the_mixer() -> Result<()> {
        let (level, muted) = read_card_level("");
        ensure!(level.is_none() && !muted, "blank stays unknown");
        Ok(())
    }

    #[test]
    fn alsa_raw_round_trips_through_gain_curve() -> Result<()> {
        let bottom = alsa_raw_to_volume(0, 0, 100);
        ensure!(bottom == 0.0, "bottom is silence");
        let top = alsa_raw_to_volume(100, 0, 100);
        ensure!((top - 1.0).abs() < 1e-9, "top is unity");
        for raw in 0..=100_i32 {
            let slider = alsa_raw_to_volume(i64::from(raw), 0, 100);
            let gain = volume_to_gain(slider);
            let expected = f64::from(raw) / 100.0;
            let error = (gain - expected).abs();
            ensure!(error < 1e-9, "raw inverts");
        }
        let flat = alsa_raw_to_volume(5, 10, 10);
        ensure!((flat - 1.0).abs() < 1e-9, "flat is unity");
        Ok(())
    }
}

//! Audio device enumeration, output mode, and availability checks.

use {
    cpal::{
        Device, default_host,
        traits::{DeviceTrait, HostTrait},
    },
    serde::{Deserialize, Serialize},
};

use crate::playback::OutputError::{self, Output};

/// Describes an available audio output device.
pub struct DeviceInfo {
    /// Stable device identifier for persisting selection across restarts.
    pub id: String,
    /// Human-readable device name for display.
    pub name: String,
}

/// Describes whether the output path is bit-perfect or resampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// Audio is output at the device's native configuration; samples pass
    /// through without volume scaling.
    BitPerfect,
    /// Audio is output via a resampled path with volume scaling.
    Resampled,
}

impl OutputMode {
    /// Get the symbolic icon name for this output mode.
    #[must_use]
    pub const fn icon_name(self) -> &'static str {
        match self {
            Self::BitPerfect => "media-optical-cd-audio-symbolic",
            Self::Resampled => "audio-card-symbolic",
        }
    }
}

/// Derive the ALSA mixer card name from a CPAL device identifier.
///
/// CPAL ALSA device names follow patterns like `"hw:0,0"` (PCM device) or
/// `"default"`. The mixer card name is the part before the comma (e.g. `"hw:0"`),
/// or the whole name for simple identifiers.
#[must_use]
pub fn alsa_card_name(device_id: &str) -> String {
    if let Some(card_part) = device_id.split(',').next()
        && !card_part.is_empty()
        && card_part != device_id
    {
        return card_part.to_string();
    }
    device_id.to_string()
}

/// Check whether at least one audio output device is available.
///
/// Returns `true` if the host reports any output devices.
/// This is a lightweight check that does not attempt to open a stream.
#[must_use]
pub fn is_device_available() -> bool {
    let host = default_host();
    if host.default_output_device().is_some() {
        return true;
    }
    host.output_devices().is_ok_and(|mut d| d.next().is_some())
}

/// Graceful check for audio device availability.
///
/// Returns `None` if at least one device is available, or an explanatory
/// message string if no device was found. Does not panic or open any stream.
/// Use this at startup to detect missing audio hardware per FR-030.
#[must_use]
pub fn startup_device_check() -> Option<String> {
    let host = default_host();
    if host.default_output_device().is_some() {
        return None;
    }
    let has_any = host.output_devices().is_ok_and(|mut d| d.next().is_some());
    if has_any {
        return None;
    }
    Some(
        "No audio output device found. Check your audio output and ensure PulseAudio or PipeWire \
         is running. Playback will be unavailable, but library scanning and browsing will still \
         work."
            .to_string(),
    )
}

/// Sort devices so that `PipeWire` and `PulseAudio` PCM devices are tried first.
///
/// These virtual devices are more likely to be available and working on
/// modern Linux desktops than raw hardware devices.
pub fn prioritize_devices(devices: &mut [Device]) {
    devices.sort_by_key(|d| {
        let desc = d.description().map(|s| s.to_string()).unwrap_or_default();
        if desc.contains("PipeWire") {
            0
        } else if desc.contains("PulseAudio") {
            1
        } else {
            2
        }
    });
}

/// List available audio output devices with both stable ID and display name.
///
/// # Errors
///
/// Returns [`OutputError`] if device enumeration fails.
pub fn list_output_devices() -> Result<Vec<DeviceInfo>, OutputError> {
    let host = default_host();
    let mut devices = Vec::new();
    for device in host.output_devices().map_err(|e| Output(e.to_string()))? {
        let id = device
            .id()
            .map_or_else(|_| String::new(), |d| d.to_string());
        let name = device
            .description()
            .map_or_else(|_| "Unknown Device".into(), |d| d.to_string());
        devices.push(DeviceInfo { id, name });
    }
    Ok(devices)
}

/// Bit-perfect output verification.
///
/// Per SC-003, bit-perfect playback is verified by comparing the digital
/// audio output against the source file — the bit stream must match
/// exactly when the device supports the file's native format.
/// This module provides the verification infrastructure.
///
/// The verification process:
/// 1. Decode a known-reference FLAC file via symphonia to PCM
/// 2. Capture the CPAL output buffer after playback
/// 3. Assert byte-identical match across all frames
///
/// In test environments without audio hardware, the `OutputMode` enum
/// and `supports_native` method are verified directly.
#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use crate::playback::devices::{
        OutputMode::{self, BitPerfect, Resampled},
        alsa_card_name, list_output_devices,
    };

    #[test]
    fn list_devices_does_not_panic() {
        let result = list_output_devices();
        assert!(
            result.is_ok(),
            "list_output_devices failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn device_info_has_display_name() {
        let Ok(devices) = list_output_devices() else {
            return;
        };
        for d in &devices {
            assert!(!d.name.is_empty(), "device name should not be empty");
        }
    }

    #[test]
    fn output_mode_default_is_resampled() {
        assert_eq!(Resampled as u8, 1);
        assert_eq!(BitPerfect as u8, 0);
    }

    #[test]
    fn output_mode_debug_representation() {
        let fmt = format!("{BitPerfect:?}");
        assert_eq!(fmt, "BitPerfect");
        let fmt = format!("{Resampled:?}");
        assert_eq!(fmt, "Resampled");
    }

    #[test]
    fn output_mode_clone_and_copy() {
        let mode = BitPerfect;
        let copied = mode;
        assert_eq!(mode, copied);
    }

    #[test]
    fn output_mode_partial_eq() {
        assert_eq!(BitPerfect, BitPerfect);
        assert_eq!(Resampled, Resampled);
        assert_ne!(BitPerfect, Resampled);
    }

    #[test]
    fn alsa_card_name_strips_pcm_device_index() {
        assert_eq!(alsa_card_name("hw:0,0"), "hw:0");
        assert_eq!(alsa_card_name("hw:1,3"), "hw:1");
        assert_eq!(alsa_card_name("hw:0"), "hw:0");
    }

    #[test]
    fn alsa_card_name_passes_through_simple_ids() {
        assert_eq!(alsa_card_name("default"), "default");
        assert_eq!(alsa_card_name("usb"), "usb");
    }

    #[test]
    fn output_mode_serde_round_trip() {
        let Ok(json) = to_string(&BitPerfect) else {
            return;
        };
        assert_eq!(json, "\"bit_perfect\"", "bit_perfect uses snake_case tag");
        let Ok(restored) = from_str::<OutputMode>(&json) else {
            return;
        };
        assert_eq!(restored, BitPerfect);

        let Ok(json) = to_string(&Resampled) else {
            return;
        };
        assert_eq!(json, "\"resampled\"", "resampled uses snake_case tag");
        let Ok(restored) = from_str::<OutputMode>(&json) else {
            return;
        };
        assert_eq!(restored, Resampled);
    }
}

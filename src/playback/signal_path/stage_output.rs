//! Signal path output stages: transport, output device, and footer card.
//!
//! Pure helpers behind
//! [`build_snapshot`](crate::playback::signal_path::path_snapshot::build_snapshot) for the bottom
//! of the chain plus the single MVP footer device. Stage construction for conversions and volume
//! lives in [`stage_build`](crate::playback::signal_path::stage_build).

use crate::playback::{
    devices::OutputMode::{BitPerfect, Resampled},
    signal_path::{
        DeviceRole::Output,
        PathStage,
        QualityVerdict::{BitPerfect as VerdictBitPerfect, Limited, Processed},
        RenderingDevice, SnapshotInput,
        StageKind::{Output as KindOutput, Transport},
        stage_build::push_stage,
    },
};

/// Resolve the device display name, falling back to the device id.
fn display_name(input: &SnapshotInput) -> String {
    if input.device_name.trim().is_empty() {
        input.device_id.clone()
    } else {
        input.device_name.clone()
    }
}

/// Check whether the output targets the shared system mixer.
fn is_shared_mixer(device_id: &str, device_name: &str) -> bool {
    if device_id == "default" || device_id.starts_with("default:") {
        return true;
    }
    let lower = device_name.to_lowercase();
    lower.contains("pipewire")
        || lower.contains("pulse")
        || lower.contains("system mixer")
        || lower.contains("default")
}

/// Check whether the output device connects over USB.
fn is_usb_device(device_id: &str, device_name: &str) -> bool {
    device_id.to_lowercase().contains("usb") || device_name.to_lowercase().contains("usb")
}

/// Append the transport stage with Linux output-mode wording.
pub fn push_transport_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) {
    let display = display_name(input);
    let lost = input.device_lost;
    let shared = is_shared_mixer(&input.device_id, &input.device_name);
    let usb = is_usb_device(&input.device_id, &input.device_name);
    let (title, mode, verdict) = if lost {
        (
            String::from("Output Device Lost"),
            String::from("disconnected"),
            Limited,
        )
    } else if shared {
        (
            String::from("ALSA Shared Output"),
            String::from("ALSA shared/system mixer"),
            Limited,
        )
    } else if usb && input.output_mode == BitPerfect {
        (
            String::from("USB Output"),
            String::from("USB output"),
            VerdictBitPerfect,
        )
    } else if usb {
        (
            String::from("USB Output"),
            String::from("USB output"),
            Processed,
        )
    } else if input.output_mode == Resampled {
        (
            String::from("ALSA Direct Output"),
            String::from("ALSA direct exclusive"),
            Processed,
        )
    } else {
        (
            String::from("ALSA Direct Output"),
            String::from("ALSA direct exclusive"),
            VerdictBitPerfect,
        )
    };
    let detail = format!("{display} ({mode})");
    let explanation = if lost {
        format!(
            "The output device {display} disappeared during playback. The path cannot claim \
             bit-perfect playback to a missing destination."
        )
    } else if shared {
        format!(
            "Routes the stream through the shared system mixer to {display}, which can change \
             levels and format."
        )
    } else if verdict == Processed {
        format!("Sends the stream to {display} through the resampled output path.")
    } else {
        format!(
            "Delivers the stream straight to {display} with exclusive access, preserving every \
             bit."
        )
    };
    push_stage(
        stages,
        Transport,
        title,
        detail,
        explanation,
        verdict,
        "audio-card-symbolic",
    );
}

/// Append the terminal Output stage for the active rendering device.
pub fn push_output_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) {
    let display = display_name(input);
    let verdict = if input.device_lost || is_shared_mixer(&input.device_id, &input.device_name) {
        Limited
    } else {
        VerdictBitPerfect
    };
    let explanation = format!("Renders the final stream on {display}.");
    push_stage(
        stages,
        KindOutput,
        String::from("Output"),
        display,
        explanation,
        verdict,
        "audio-speakers-symbolic",
    );
}

/// Build the single MVP footer device from the active output facts.
#[must_use]
pub fn output_device(input: &SnapshotInput) -> RenderingDevice {
    RenderingDevice {
        display_name: display_name(input),
        role: Output,
        brand_visual: String::from("audio-card-symbolic"),
        illustration: String::from("audio-speakers-symbolic"),
        manual_url: None,
    }
}

/// Build the shared Lab DAC device for tests.
///
/// # Returns
///
/// * `RenderingDevice` - Deterministic output device for unit tests.
#[must_use]
pub fn lab_test_device() -> RenderingDevice {
    RenderingDevice {
        display_name: String::from("Test DAC"),
        role: Output,
        brand_visual: String::from("audio-card-symbolic"),
        illustration: String::from("audio-speakers-symbolic"),
        manual_url: None,
    }
}

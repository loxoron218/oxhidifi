//! Device and system volume stages: hardware and OS-mixer attenuation.
//!
//! Constructors behind
//! [`build_snapshot`](crate::playback::signal_path::path_snapshot::build_snapshot) for the two
//! non-DSP volume families. DSP scaling stays in
//! [`stage_build`](crate::playback::signal_path::stage_build); every family
//! emits only while attenuating, ordered DSP, device, system just before
//! Transport. Device attenuation is `Processed` without forcing `Limited`;
//! system attenuation is always `Limited`.

use crate::playback::{
    alsa_mixer::SystemMixerSource::{AlsaShared, PipeWirePulse, Unknown},
    devices::OutputMode::BitPerfect,
    signal_path::{
        PathStage,
        QualityVerdict::{Limited, Processed},
        SnapshotInput,
        StageKind::{SystemVolume, Volume},
        stage_build::push_stage,
        stage_output::is_shared_mixer,
    },
    state::MuteState::Muted,
    volume::format_volume_db,
};

/// Format one volume level as dB, or `Muted (...)` when muted.
///
/// # Arguments
///
/// * `volume` - Linear slider value from 0.0 to 1.0.
/// * `muted` - Whether the mute switch is engaged.
///
/// # Returns
///
/// * `String` - dB detail, or `Muted (...)` when muted.
fn level_detail(volume: f64, muted: bool) -> String {
    let db = format_volume_db(volume);
    if muted { format!("Muted ({db})") } else { db }
}

/// Append the device/hardware volume stage in `BitPerfect` mode only.
///
/// Uses the ALSA read-back level when available, else the engine-set slider
/// value (the engine drove that hardware level, so slider truth is exact).
/// Hardware attenuation is `Processed` without forcing `Limited` and never
/// arms the processing-speed readout.
///
/// # Arguments
///
/// * `stages` - Chain under construction; the stage lands before Transport.
/// * `input` - Snapshot facts carrying the device level and mute state.
pub fn push_device_volume_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) {
    if input.output_mode != BitPerfect {
        return;
    }
    let level = input.device_volume.unwrap_or(input.volume);
    let muted = input.device_muted || input.muted == Muted;
    if level >= 1.0 && !muted {
        return;
    }
    let detail = level_detail(level, muted);
    let explanation = format!(
        "Controls the ALSA hardware mixer (Master/PCM element) at {detail}. Attenuation here \
         changes the output level without in-app processing."
    );
    push_stage(
        stages,
        Volume,
        String::from("Device Volume"),
        detail,
        explanation,
        Processed,
        "audio-volume-high-symbolic",
    );
}

/// Append the system/application volume stage for OS-mixer attenuation.
///
/// Emits with the dB level when readable, the standing `unknown` disclosure
/// on shared-mixer transports when unreadable, and nothing at unity or on
/// exclusive paths with no readable level. Always `Limited` and never arms
/// the processing-speed readout.
///
/// # Arguments
///
/// * `stages` - Chain under construction; the stage lands before Transport.
/// * `input` - Snapshot facts carrying the system level and mute state.
pub fn push_system_volume_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) {
    let shared = is_shared_mixer(&input.device_id, &input.device_name);
    let detail = if let Some(level) = input.system_volume {
        if level >= 1.0 && !input.system_muted {
            return;
        }
        level_detail(level, input.system_muted)
    } else {
        if !shared {
            return;
        }
        if input.system_muted {
            String::from("Muted (unknown)")
        } else {
            String::from("unknown")
        }
    };
    let source = match input.system_source {
        AlsaShared => "the ALSA shared/system mixer",
        PipeWirePulse => "the PipeWire/PulseAudio system mixer",
        Unknown => "the operating-system mixer",
    };
    let explanation = format!(
        "Reflects the OS-mixer level outside the app ({source}) at {detail}. The shared mixer can \
         change levels behind the app, so this path cannot stay bit-perfect."
    );
    push_stage(
        stages,
        SystemVolume,
        String::from("System Volume"),
        detail,
        explanation,
        Limited,
        "audio-volume-medium-symbolic",
    );
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail, ensure};

    use crate::playback::{
        alsa_mixer::SystemMixerSource::{AlsaShared, PipeWirePulse, Unknown},
        devices::OutputMode::{BitPerfect, Resampled},
        signal_path::{
            PathStage,
            QualityVerdict::{Limited, Processed},
            SnapshotInput,
            stage_volume::{push_device_volume_stage, push_system_volume_stage},
        },
        state::{MuteState::Unmuted, PlaybackStatus::Playing},
        volume::format_volume_db,
    };

    fn volume_input() -> SnapshotInput {
        SnapshotInput {
            generation: 2,
            track_id: Some(10),
            zone_name: String::from("Spare DAC"),
            auth: None,
            device_id: String::from("hw:2"),
            device_name: String::from("Spare DAC"),
            device_sample_rate: 48000,
            device_channels: 2,
            device_lost: false,
            device_volume: None,
            device_muted: false,
            system_volume: None,
            system_muted: false,
            system_source: Unknown,
            volume: 1.0,
            muted: Unmuted,
            output_mode: BitPerfect,
            status: Playing,
            track_audio: None,
            decoder_params: None,
            resampler_in_rate: None,
            resampler_out_rate: None,
            resampler_channels: None,
            sampled_at_wall: 0,
            decoded_frames: 0,
            resampled_frames: 0,
        }
    }

    fn emit_device(input: &SnapshotInput) -> Vec<PathStage> {
        let mut stages = Vec::new();
        push_device_volume_stage(&mut stages, input);
        stages
    }

    fn emit_system(input: &SnapshotInput) -> Vec<PathStage> {
        let mut stages = Vec::new();
        push_system_volume_stage(&mut stages, input);
        stages
    }

    #[test]
    fn device_volume_reads_back_db_without_limiting() -> Result<()> {
        let mut input = volume_input();
        input.device_volume = Some(0.5);
        let stages = emit_device(&input);
        ensure!(stages.len() == 1, "read-back attenuation emits one stage");
        let Some(stage) = stages.first() else {
            bail!("device stage must exist")
        };
        ensure!(stage.title == "Device Volume", "device keeps its title");
        ensure!(
            stage.detail == format_volume_db(0.5),
            "device carries dB, got {}",
            stage.detail
        );
        ensure!(
            stage.verdict == Processed,
            "device attenuation stays Processed"
        );
        ensure!(!stage.explanation.is_empty(), "explanation never empty");
        Ok(())
    }

    #[test]
    fn device_volume_falls_back_to_slider_and_mute() -> Result<()> {
        let mut input = volume_input();
        input.volume = 0.5;
        let stages = emit_device(&input);
        ensure!(stages.len() == 1, "slider fallback emits when unreadable");
        let Some(stage) = stages.first() else {
            bail!("fallback stage must exist")
        };
        ensure!(
            stage.detail == format_volume_db(0.5),
            "fallback carries dB, got {}",
            stage.detail
        );
        let mut muted = volume_input();
        muted.device_muted = true;
        let stages = emit_device(&muted);
        let Some(stage) = stages.first() else {
            bail!("muted hardware must emit")
        };
        ensure!(
            stage.detail.starts_with("Muted ("),
            "muted hardware names muting, got {}",
            stage.detail
        );
        let unity = emit_device(&volume_input());
        ensure!(unity.is_empty(), "unity hardware emits nothing");
        let mut resampled = volume_input();
        resampled.output_mode = Resampled;
        resampled.volume = 0.5;
        ensure!(
            emit_device(&resampled).is_empty(),
            "device stage never emits off BitPerfect"
        );
        Ok(())
    }

    #[test]
    fn system_volume_reports_db_or_unknown_as_limited() -> Result<()> {
        let mut input = volume_input();
        input.device_id = String::from("default");
        input.device_name = String::from("Default Output");
        input.system_volume = Some(0.5);
        input.system_source = AlsaShared;
        let stages = emit_system(&input);
        ensure!(stages.len() == 1, "readable system level emits one stage");
        let Some(stage) = stages.first() else {
            bail!("system stage must exist")
        };
        ensure!(stage.title == "System Volume", "system keeps its title");
        ensure!(
            stage.detail == format_volume_db(0.5),
            "system carries dB, got {}",
            stage.detail
        );
        ensure!(stage.verdict == Limited, "system attenuation is Limited");
        let mut unknown = input.clone();
        unknown.system_volume = None;
        unknown.system_source = Unknown;
        let stages = emit_system(&unknown);
        let Some(stage) = stages.first() else {
            bail!("unreadable shared mixer must disclose")
        };
        ensure!(stage.detail == "unknown", "disclosure reports unknown");
        let mut unity = input;
        unity.system_volume = Some(1.0);
        ensure!(emit_system(&unity).is_empty(), "unity system emits nothing");
        let mut exclusive = volume_input();
        exclusive.system_volume = None;
        ensure!(
            emit_system(&exclusive).is_empty(),
            "exclusive paths disclose nothing"
        );
        Ok(())
    }

    #[test]
    fn system_volume_reports_native_pipewire_level_as_limited() -> Result<()> {
        let mut input = volume_input();
        input.device_id = String::from("default");
        input.device_name = String::from("PipeWire Output");
        input.system_volume = Some(0.0);
        input.system_muted = true;
        input.system_source = PipeWirePulse;
        let stages = emit_system(&input);
        let Some(stage) = stages.first() else {
            bail!("native system level must emit")
        };
        ensure!(stage.title == "System Volume", "native keeps its title");
        ensure!(
            stage.detail.starts_with("Muted ("),
            "native mute names muting, got {}",
            stage.detail
        );
        ensure!(stage.verdict == Limited, "native attenuation is Limited");
        ensure!(
            stage.explanation.contains("PipeWire"),
            "native names its mixer, got {}",
            stage.explanation
        );
        Ok(())
    }
}

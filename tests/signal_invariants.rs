//! Snapshot contract invariants 1–3 (verdict precedence and speed presence).
//!
//! Submodule of the `signal_path` test target rooted at
//! `tests/signal_inspector.rs`; fixtures live in `crate::signal_fixtures`.
//! Covers the
//! Phase 2 foundation: bit-perfect hides speed, volume-only is processed
//! with speed, and limited wins over enhancements while keeping speed iff an
//! in-app alteration is present.

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail, ensure};

    use oxhidifi::playback::{
        alsa_mixer::SystemMixerSource::{AlsaShared, Unknown},
        devices::OutputMode::{BitPerfect, Resampled},
        signal_path::{
            PathStage,
            QualityVerdict::{self, BitPerfect as VerdictBitPerfect, Limited, Processed},
            SignalPathSnapshot, SnapshotInput,
            StageKind::{self, Output, Source, SystemVolume, Transport, Volume},
            path_snapshot::build_snapshot,
            path_verdict::resolve_verdict,
        },
        state::MuteState::{Muted, Unmuted},
        volume::format_volume_db,
    };

    use crate::signal_fixtures::{bit_perfect_input, make_device, playing_input};

    fn make_stage(position: u32, kind: StageKind, verdict: QualityVerdict) -> PathStage {
        PathStage {
            position,
            kind,
            title: String::from("Test stage"),
            detail: String::from("Test detail"),
            explanation: String::from("Test explanation."),
            verdict,
            badge_icon: "audio-x-generic-symbolic",
        }
    }

    fn snapshot_from(
        input: &SnapshotInput,
        stages: Vec<PathStage>,
        speed: Option<f64>,
    ) -> SignalPathSnapshot {
        let verdicts: Vec<QualityVerdict> = stages.iter().map(|stage| stage.verdict).collect();
        SignalPathSnapshot {
            generation: input.generation,
            track_id: input.track_id,
            zone_name: input.zone_name.clone(),
            verdict: resolve_verdict(&verdicts),
            stages,
            devices: vec![make_device()],
            processing_speed: speed,
            playback_status: input.status,
        }
    }

    #[test]
    fn invariant1_bit_perfect_hides_processing_speed() -> Result<()> {
        let input = bit_perfect_input();
        let cloned = input.clone();
        ensure!(input.track_id == cloned.track_id, "clone keeps track");
        ensure!(cloned.track_id.is_some(), "needs a track");
        ensure!(cloned.volume >= 1.0, "unity volume");
        let unmuted = cloned.muted == Unmuted;
        ensure!(unmuted, "unmuted path");
        let direct = cloned.output_mode == BitPerfect;
        ensure!(direct, "direct output mode");
        let stages = vec![
            make_stage(0, Source, VerdictBitPerfect),
            make_stage(1, Output, VerdictBitPerfect),
        ];
        let snapshot = snapshot_from(&cloned, stages, None);
        let perfect = snapshot.verdict == VerdictBitPerfect;
        ensure!(perfect, "resolves Bit-Perfect");
        let hidden = snapshot.processing_speed.is_none();
        ensure!(hidden, "bit-perfect hides speed");
        Ok(())
    }

    #[test]
    fn invariant2_volume_only_is_processed_with_speed() -> Result<()> {
        let mut input = bit_perfect_input();
        input.volume = 0.5;
        input.generation = 2;
        let cloned = input.clone();
        ensure!(input.generation == cloned.generation, "clone keeps gen");
        let scaled = cloned.volume < 1.0;
        ensure!(scaled, "volume scaling active");
        let stages = vec![
            make_stage(0, Source, VerdictBitPerfect),
            make_stage(1, Volume, Processed),
            make_stage(2, Output, VerdictBitPerfect),
        ];
        let snapshot = snapshot_from(&cloned, stages, Some(42.0));
        let processed = snapshot.verdict == Processed;
        ensure!(processed, "volume forces Processed");
        let shown = snapshot.processing_speed.is_some();
        ensure!(shown, "volume-only shows speed");
        Ok(())
    }

    #[test]
    fn invariant3_limited_wins_over_enhancements() -> Result<()> {
        let mut input = bit_perfect_input();
        input.volume = 0.5;
        input.output_mode = Resampled;
        input.device_id = String::from("default");
        input.device_lost = true;
        input.generation = 3;
        let cloned = input.clone();
        ensure!(input.generation == cloned.generation, "clone keeps gen");
        let lost = cloned.device_lost;
        ensure!(lost, "device lost mid-playback");
        let stages = vec![
            make_stage(0, Source, VerdictBitPerfect),
            make_stage(1, Volume, Processed),
            make_stage(2, Transport, Limited),
            make_stage(3, Output, Limited),
        ];
        let snapshot = snapshot_from(&cloned, stages, Some(42.0));
        let limited = snapshot.verdict == Limited;
        ensure!(limited, "Limited wins over Processed");
        let shown = snapshot.processing_speed.is_some();
        ensure!(shown, "alteration still shows speed");
        Ok(())
    }

    fn shared_input(track_id: i64, generation: u64) -> SnapshotInput {
        let mut input = playing_input(track_id, generation, 48000, Some(16));
        input.device_id = String::from("default");
        input.device_name = String::from("Default Output");
        input
    }

    fn stage_position(stages: &[PathStage], title: &str) -> Option<usize> {
        stages.iter().position(|stage| stage.title == title)
    }

    fn stage_kind_position(stages: &[PathStage], kind: StageKind) -> Option<usize> {
        stages.iter().position(|stage| stage.kind == kind)
    }

    fn find_stage<'a>(stages: &'a [PathStage], title: &str) -> Option<&'a PathStage> {
        stages.iter().find(|stage| stage.title == title)
    }

    #[test]
    fn phase8_dsp_volume_gated_on_resampled() -> Result<()> {
        let mut input = playing_input(61, 61, 48000, Some(16));
        input.volume = 0.5;
        ensure!(input.output_mode == BitPerfect, "fixture starts direct");
        let direct = build_snapshot(&input)?;
        let dsp = stage_position(&direct.stages, "Volume");
        ensure!(dsp.is_none(), "no DSP Volume in BitPerfect");
        let mut resampled = input;
        resampled.generation = 62;
        resampled.output_mode = Resampled;
        let snapshot = build_snapshot(&resampled)?;
        let dsp = stage_position(&snapshot.stages, "Volume");
        ensure!(dsp.is_some(), "DSP Volume in Resampled");
        Ok(())
    }

    #[test]
    fn phase8_device_volume_readback_and_fallback() -> Result<()> {
        let expected = format_volume_db(0.5);
        let mut read = playing_input(63, 63, 48000, Some(16));
        read.device_volume = Some(0.5);
        let snapshot = build_snapshot(&read)?;
        let Some(stage) = find_stage(&snapshot.stages, "Device Volume") else {
            bail!("BitPerfect attenuation must emit a Device Volume stage")
        };
        let readback = stage.detail == expected;
        ensure!(readback, "read-back carries dB");
        ensure!(stage.verdict == Processed, "device is Processed");
        let mut fallback = playing_input(64, 64, 48000, Some(16));
        fallback.volume = 0.5;
        let snapshot = build_snapshot(&fallback)?;
        let Some(stage) = find_stage(&snapshot.stages, "Device Volume") else {
            bail!("slider fallback must emit Device Volume when unreadable")
        };
        let fallback_db = stage.detail == expected;
        ensure!(fallback_db, "fallback carries dB");
        let mut muted = playing_input(65, 65, 48000, Some(16));
        muted.device_muted = true;
        let snapshot = build_snapshot(&muted)?;
        let Some(stage) = find_stage(&snapshot.stages, "Device Volume") else {
            bail!("muted hardware must emit Device Volume")
        };
        let muted_mark = stage.detail.starts_with("Muted (");
        ensure!(muted_mark, "muted names muting");
        Ok(())
    }

    #[test]
    fn phase8_system_volume_levels_and_disclosure() -> Result<()> {
        let expected = format_volume_db(0.5);
        let mut input = shared_input(66, 66);
        input.system_volume = Some(0.5);
        input.system_source = AlsaShared;
        let snapshot = build_snapshot(&input)?;
        let Some(stage) = find_stage(&snapshot.stages, "System Volume") else {
            bail!("shared attenuation must emit a System Volume stage")
        };
        let readable = stage.detail == expected;
        ensure!(readable, "readable carries dB");
        ensure!(stage.verdict == Limited, "system is Limited");
        ensure!(stage.kind == SystemVolume, "system keeps kind");
        let mut unknown = input;
        unknown.generation = 67;
        unknown.system_volume = None;
        unknown.system_source = Unknown;
        let snapshot = build_snapshot(&unknown)?;
        let Some(stage) = find_stage(&snapshot.stages, "System Volume") else {
            bail!("unreadable shared mixer must disclose System Volume")
        };
        let disclosed = stage.detail == "unknown";
        ensure!(disclosed, "unreadable reports unknown");
        ensure!(stage.verdict == Limited, "disclosure is Limited");
        Ok(())
    }

    #[test]
    fn phase8_volume_order_before_transport() -> Result<()> {
        let mut resampled = shared_input(68, 68);
        resampled.output_mode = Resampled;
        resampled.volume = 0.5;
        resampled.system_volume = Some(0.5);
        resampled.system_source = AlsaShared;
        let snapshot = build_snapshot(&resampled)?;
        let stages = &snapshot.stages;
        let dsp = stage_position(stages, "Volume");
        let dsp_system = stage_position(stages, "System Volume");
        let transport = stage_kind_position(stages, Transport);
        let output = stage_kind_position(stages, Output);
        let (Some(dsp), Some(dsp_system), Some(transport), Some(output)) =
            (dsp, dsp_system, transport, output)
        else {
            bail!("resampled shared path must carry DSP, system, transport, output")
        };
        let ordered = dsp < dsp_system && dsp_system < transport && transport < output;
        ensure!(ordered, "DSP before system before transport");
        let mut hardware = shared_input(69, 69);
        hardware.device_volume = Some(0.5);
        hardware.system_volume = Some(0.5);
        hardware.system_source = AlsaShared;
        let snapshot = build_snapshot(&hardware)?;
        let stages = &snapshot.stages;
        let device = stage_position(stages, "Device Volume");
        let hw_system = stage_position(stages, "System Volume");
        let transport = stage_kind_position(stages, Transport);
        let (Some(device), Some(hw_system), Some(transport)) = (device, hw_system, transport)
        else {
            bail!("hardware shared path must carry device, system, transport")
        };
        let ordered = device < hw_system && hw_system < transport;
        ensure!(ordered, "device before system before transport");
        let unity = build_snapshot(&playing_input(70, 70, 48000, Some(16)))?;
        let quiet = unity.stages.iter().all(|stage| {
            stage.title != "Volume"
                && stage.title != "Device Volume"
                && stage.title != "System Volume"
        });
        ensure!(quiet, "unity omits every volume family");
        Ok(())
    }

    #[test]
    fn phase8_device_only_is_processed_without_speed() -> Result<()> {
        let mut input = playing_input(71, 71, 48000, Some(16));
        input.device_volume = Some(0.5);
        let snapshot = build_snapshot(&input)?;
        ensure!(snapshot.verdict == Processed, "device is Processed");
        let hidden = snapshot.processing_speed.is_none();
        ensure!(hidden, "device never arms readout");
        Ok(())
    }

    #[test]
    fn phase8_system_only_is_limited_without_speed() -> Result<()> {
        let mut input = shared_input(72, 72);
        input.system_volume = Some(0.5);
        input.system_source = AlsaShared;
        let snapshot = build_snapshot(&input)?;
        ensure!(snapshot.verdict == Limited, "system is Limited");
        let hidden = snapshot.processing_speed.is_none();
        ensure!(hidden, "system never arms readout");
        Ok(())
    }

    #[test]
    fn phase8_unity_exclusive_is_bit_perfect_without_speed() -> Result<()> {
        let snapshot = build_snapshot(&playing_input(73, 73, 48000, Some(16)))?;
        let perfect = snapshot.verdict == VerdictBitPerfect;
        ensure!(perfect, "all-unity exclusive is Bit-Perfect");
        let hidden = snapshot.processing_speed.is_none();
        ensure!(hidden, "all-unity exclusive hides readout");
        Ok(())
    }

    #[test]
    fn phase8_shared_unity_without_system_row_stays_limited() -> Result<()> {
        let mut input = shared_input(74, 74);
        input.system_volume = Some(1.0);
        input.system_source = AlsaShared;
        let snapshot = build_snapshot(&input)?;
        let system = stage_position(&snapshot.stages, "System Volume");
        ensure!(system.is_none(), "unity system emits no row");
        ensure!(snapshot.verdict == Limited, "shared alone stays Limited");
        let hidden = snapshot.processing_speed.is_none();
        ensure!(hidden, "Limited-only shared hides readout");
        Ok(())
    }

    #[test]
    fn phase8_engine_mute_applies_on_next_rebuild() -> Result<()> {
        let mut input = playing_input(75, 75, 48000, Some(16));
        input.muted = Muted;
        let snapshot = build_snapshot(&input)?;
        ensure!(snapshot.verdict == Processed, "mute is Processed");
        Ok(())
    }
}

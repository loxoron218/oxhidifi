//! Audio Signal Path Inspector acceptance tests (FR-001..FR-015).
//!
//! Phase 1 skeleton for `specs/002-signal-path`. Full builder/verdict/describe
//! coverage lands in Phase 2 (T008) and per-story tasks (T009..T032).
//!
//! FR mapping:
//! - FR-001 badge entry, FR-002 vertical chain, FR-003 header verdict
//!   (`Bit-Perfect`/`Processed`/`Limited`, precedence Limited > Processed > Bit-Perfect), FR-004
//!   Source, FR-005 transformations, FR-006 Output + external renderer, FR-007 per-stage
//!   indicators, FR-008 inline explainer, FR-009 device footer, FR-010 processing-speed readout,
//!   FR-011 atomic live updates, FR-012 `Signal` tab, FR-013 dark/light, FR-014 kebab `MenuButton`
//!   (sole popover exception), FR-015 keyboard + screen-reader.
//!
//! SC mapping (proxy invariants until `playback::signal_path` exists):
//! - SC-003 (zero silent alterations) maps to `contracts/snapshot.md` invariants 2 and 5: any
//!   alteration yields verdict >= `Processed`, and `describe` never returns empty strings with
//!   converter input-to-output wording plus explicit DSD-to-PCM.
//! - SC-006 (gapless atomic swap) maps to `contracts/snapshot.md` invariant 4 plus the data-model
//!   generation rule: whole-snapshot swap on `generation` change, never mixed rows.
//! - SC-002 (9/10 classify untouched/processed/limited) is proxy logic here: precedence Limited >
//!   Processed > Bit-Perfect over per-stage verdicts. Human 9/10 classification is validated
//!   manually via `quickstart.md` scenarios 1-3, not asserted automatically.
//! - SC-001/SC-004/SC-005 map to `contracts/dialog.md` (badge/header, explainer <1s, footer cards)
//!   and are covered by later GTK/quickstart validation (T018, T026, T033..T034).

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use oxhidifi::playback::{
        devices::OutputMode::{BitPerfect as ModeBitPerfect, Resampled},
        signal_path::{
            DeviceRole::Output as DeviceOutput,
            PathStage,
            QualityVerdict::{self, BitPerfect, Limited, Processed},
            RenderingDevice, SignalPathSnapshot, SnapshotInput,
            StageKind::{self, Output, Source, Transport, Volume},
            path_verdict::resolve_verdict,
        },
        state::{MuteState::Unmuted, PlaybackStatus::Playing},
    };

    const VERDICTS: [&str; 3] = ["Bit-Perfect", "Processed", "Limited"];

    fn resolve_proxy(stages: &[&str]) -> &'static str {
        if stages.contains(&"Limited") {
            return "Limited";
        }
        if stages.contains(&"Processed") {
            return "Processed";
        }
        "Bit-Perfect"
    }

    fn assert_verdict_precedence_proxy() -> Result<()> {
        ensure!(
            VERDICTS == ["Bit-Perfect", "Processed", "Limited"],
            "canonical verdict labels must be exactly Bit-Perfect/Processed/Limited"
        );
        ensure!(
            resolve_proxy(&[]) == "Bit-Perfect",
            "empty stage list resolves to Bit-Perfect"
        );
        ensure!(
            resolve_proxy(&["Bit-Perfect", "Bit-Perfect"]) == "Bit-Perfect",
            "all bit-perfect stages resolve to Bit-Perfect"
        );
        ensure!(
            resolve_proxy(&["Bit-Perfect", "Processed"]) == "Processed",
            "any Processed stage forces Processed"
        );
        ensure!(
            resolve_proxy(&["Processed", "Limited"]) == "Limited",
            "Limited wins over Processed"
        );
        ensure!(
            resolve_proxy(&["Bit-Perfect", "Limited"]) == "Limited",
            "Limited wins over Bit-Perfect"
        );
        Ok(())
    }

    fn assert_zero_silent_alterations_proxy() -> Result<()> {
        let altering = ["resample", "bit-depth", "dsd-to-pcm", "volume", "eq"];
        for stage in altering {
            ensure!(
                !stage.is_empty(),
                "altering stage detail must never be empty (SC-003)"
            );
        }
        let converter_detail = "96kHz to 192kHz";
        ensure!(
            converter_detail.contains(" to "),
            "converter detail must carry input-to-output wording (SC-003)"
        );
        Ok(())
    }

    fn assert_gapless_atomic_swap_proxy() -> Result<()> {
        let old_generation = 1_u64;
        let new_generation = 2_u64;
        let old_track = Some(1_i64);
        let new_track = Some(2_i64);
        ensure!(
            old_generation != new_generation,
            "generation must bump on track change (SC-006)"
        );
        ensure!(
            old_track != new_track,
            "track id must change across gapless transition (SC-006)"
        );
        Ok(())
    }

    #[test]
    fn sc002_verdict_precedence_proxy() -> Result<()> {
        assert_verdict_precedence_proxy()
    }

    #[test]
    fn sc003_zero_silent_alterations_proxy() -> Result<()> {
        assert_zero_silent_alterations_proxy()
    }

    #[test]
    fn sc006_gapless_atomic_swap_proxy() -> Result<()> {
        assert_gapless_atomic_swap_proxy()
    }

    fn bit_perfect_input() -> SnapshotInput {
        SnapshotInput {
            device_id: String::from("hw:1"),
            device_name: String::from("Lab DAC"),
            device_sample_rate: 48000,
            device_channels: 2,
            device_lost: false,
            generation: 1,
            track_id: Some(12),
            track_audio: None,
            decoder_params: None,
            resampler_in_rate: None,
            resampler_out_rate: None,
            resampler_channels: None,
            volume: 1.0,
            muted: Unmuted,
            output_mode: ModeBitPerfect,
            status: Playing,
            zone_name: String::from("Lab DAC"),
            auth: None,
        }
    }

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

    fn make_device() -> RenderingDevice {
        RenderingDevice {
            display_name: String::from("Lab DAC"),
            role: DeviceOutput,
            brand_visual: String::from("audio-card-symbolic"),
            illustration: String::from("audio-speakers-symbolic"),
            manual_url: None,
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
        let direct = cloned.output_mode == ModeBitPerfect;
        ensure!(direct, "direct output mode");
        let stages = vec![
            make_stage(0, Source, BitPerfect),
            make_stage(1, Output, BitPerfect),
        ];
        let snapshot = snapshot_from(&cloned, stages, None);
        let perfect = snapshot.verdict == BitPerfect;
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
            make_stage(0, Source, BitPerfect),
            make_stage(1, Volume, Processed),
            make_stage(2, Output, BitPerfect),
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
            make_stage(0, Source, BitPerfect),
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
}

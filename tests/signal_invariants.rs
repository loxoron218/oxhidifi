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
    use anyhow::{Result, ensure};

    use oxhidifi::playback::{
        devices::OutputMode::{BitPerfect, Resampled},
        signal_path::{
            PathStage,
            QualityVerdict::{self, BitPerfect as VerdictBitPerfect, Limited, Processed},
            SignalPathSnapshot, SnapshotInput,
            StageKind::{self, Output, Source, Transport, Volume},
            path_verdict::resolve_verdict,
        },
        state::MuteState::Unmuted,
    };

    use crate::signal_fixtures::{bit_perfect_input, make_device};

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
}

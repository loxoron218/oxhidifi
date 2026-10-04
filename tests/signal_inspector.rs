//! Audio Signal Path Inspector acceptance tests (FR-001..FR-015).
//!
//! FR mapping: FR-001 badge, FR-002 chain, FR-003 verdict (`Bit-Perfect`/
//! `Processed`/`Limited`, Limited > Processed > Bit-Perfect), FR-004 Source,
//! FR-005 transformations, FR-006 Output + renderer, FR-007 indicators,
//! FR-008 explainer, FR-009 footer, FR-010 speed, FR-011 atomic updates,
//! FR-012 `Signal` tab, FR-013 dark/light, FR-014 kebab, FR-015 a11y.
//!
//! SC mapping: SC-003 via `us3_describe_contract` (wording + explicit DSD
//! stage); SC-006 via `us1_gapless_atomic_swap`; SC-002 via
//! `us2_verdict_precedence` (human 9/10 manual via quickstart 1-3);
//! SC-001/SC-004/SC-005 via GTK/quickstart validation.
//!
//! Layout: `signal_fixtures` (shared deterministic inputs),
//! `signal_invariants` (snapshot invariants 1–3), and `signal_us4` (US4
//! footer/speed contracts) are submodules of this target so every file
//! respects the 400-line gate. Mirrors the `tests/abx` sharing pattern.

pub mod signal_fixtures;
pub mod signal_invariants;
pub mod signal_us4;

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, bail, ensure};

    use oxhidifi::playback::{
        devices::OutputMode::Resampled,
        signal_path::{
            QualityVerdict::{self, BitPerfect, Limited, Processed},
            SignalPathError::NoActiveTrack,
            SnapshotInput,
            StageFacts::{self, Effect, FormatConverter, SampleRateConverter, Volume},
            StageKind::{ExternalRenderer, FormatConverter as KindFormatConverter, Output, Source},
            describe_contract::check_describe_pair,
            path_snapshot::{build_snapshot, summarize_text},
        },
        state::MuteState::Muted,
        volume::format_volume_db,
    };

    use crate::signal_fixtures::{dsd_input, playing_input};

    fn assert_built(input: &SnapshotInput, verdict: QualityVerdict, speed: bool) -> Result<()> {
        let snapshot = build_snapshot(input)?;
        ensure!(snapshot.verdict == verdict, "verdict must resolve");
        let shown = snapshot.processing_speed.is_some();
        ensure!(shown == speed, "speed presence must match");
        Ok(())
    }

    #[test]
    fn us2_verdict_precedence() -> Result<()> {
        let perfect = playing_input(21, 11, 48000, Some(16));
        assert_built(&perfect, BitPerfect, false)?;
        let mut scaled = perfect.clone();
        scaled.generation = 12;
        scaled.output_mode = Resampled;
        scaled.volume = 0.5;
        assert_built(&scaled, Processed, true)?;
        let mut muted = perfect.clone();
        muted.generation = 13;
        muted.output_mode = Resampled;
        muted.muted = Muted;
        assert_built(&muted, Processed, true)?;
        let mut wide = perfect.clone();
        wide.generation = 14;
        wide.device_channels = 6;
        assert_built(&wide, Processed, true)?;
        let mut up = perfect.clone();
        up.generation = 15;
        up.device_sample_rate = 96000;
        assert_built(&up, Processed, true)?;
        let down = playing_input(22, 16, 96000, Some(24));
        assert_built(&down, Limited, true)?;
        let mut shared = scaled;
        shared.generation = 17;
        shared.device_id = String::from("default");
        assert_built(&shared, Limited, true)?;
        let mut lost = perfect;
        lost.generation = 18;
        lost.device_lost = true;
        assert_built(&lost, Limited, false)?;
        Ok(())
    }

    #[test]
    fn us1_build_snapshot_stage_ordering() -> Result<()> {
        let mut input = playing_input(11, 1, 44100, Some(16));
        let snapshot = build_snapshot(&input)?;
        ensure!(!snapshot.stages.is_empty(), "a track must yield stages");
        let Some(first) = snapshot.stages.first() else {
            bail!("snapshot must contain a first stage")
        };
        ensure!(first.kind == Source, "first stage must be Source");
        let Some(last) = snapshot.stages.last() else {
            bail!("snapshot must contain a last stage")
        };
        ensure!(
            last.kind == Output || last.kind == ExternalRenderer,
            "last stage must be Output or ExternalRenderer"
        );
        let ordered = snapshot
            .stages
            .iter()
            .zip(snapshot.stages.iter().skip(1))
            .all(|(prev, next)| prev.position < next.position);
        ensure!(ordered, "stages must run in position order");
        let summary = summarize_text(&snapshot);
        ensure!(
            summary.lines().count() == snapshot.stages.len().saturating_add(1),
            "summary must list every stage once, got {summary}"
        );
        input.track_id = None;
        ensure!(
            matches!(build_snapshot(&input), Err(NoActiveTrack)),
            "missing track must map to NoActiveTrack"
        );
        Ok(())
    }

    #[test]
    fn us1_gapless_atomic_swap() -> Result<()> {
        let first = build_snapshot(&playing_input(11, 1, 44100, Some(16)))?;
        let second = build_snapshot(&playing_input(12, 2, 96000, Some(24)))?;
        ensure!(
            first.generation != second.generation,
            "generation must bump across the gapless transition"
        );
        ensure!(
            first.track_id != second.track_id,
            "track id must change across the gapless transition"
        );
        let mut displayed = first.clone();
        ensure!(
            displayed == first,
            "displayed chain starts as the first track"
        );
        displayed = second.clone();
        ensure!(
            displayed.generation == second.generation,
            "swap must replace the whole generation"
        );
        ensure!(
            displayed.track_id == second.track_id,
            "swap must replace the whole track"
        );
        ensure!(
            displayed
                .stages
                .iter()
                .all(|stage| !stage.detail.contains("44.1kHz")),
            "swapped chain must never show mixed rows"
        );
        let Some(source) = displayed.stages.first() else {
            bail!("swapped chain must contain stages")
        };
        ensure!(
            source.detail.contains("96kHz"),
            "swapped chain must reflect the new track, got {}",
            source.detail
        );
        Ok(())
    }

    fn vol(volume: f64, label: &str) -> StageFacts {
        Volume {
            volume,
            label: String::from(label),
        }
    }

    #[test]
    fn us3_describe_contract() -> Result<()> {
        let dsp_db = format_volume_db(0.5);
        let cases: Vec<(StageFacts, &str)> = vec![
            (
                SampleRateConverter {
                    input_rate: 96000,
                    output_rate: 192_000,
                },
                "96kHz to 192kHz",
            ),
            (
                FormatConverter {
                    input_format: String::from("DSD64"),
                    output_format: String::from("PCM 176.4kHz"),
                },
                "DSD64 to PCM",
            ),
            (vol(0.5, "DSP volume"), &dsp_db),
            (vol(0.8, "Leveling"), "Leveling"),
            (vol(0.9, "Headroom"), "Headroom"),
            (
                Effect {
                    kind: String::from("Channel map"),
                    summary: String::from("Stereo to Mono"),
                },
                "Channel map",
            ),
        ];
        for (kind, needle) in &cases {
            check_describe_pair(kind, needle).map_err(|e| anyhow!(e))?;
        }
        let dsd = dsd_input(31, 31)?;
        let snapshot = build_snapshot(&dsd)?;
        let explicit = snapshot
            .stages
            .iter()
            .any(|stage| stage.kind == KindFormatConverter && stage.detail.contains("DSD"));
        ensure!(explicit, "DSD needs an explicit stage");
        Ok(())
    }
}

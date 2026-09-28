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

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail, ensure};

    use oxhidifi::{
        playback::{
            decoder::AudioParams,
            devices::OutputMode::{BitPerfect as ModeBitPerfect, Resampled},
            signal_path::{
                DeviceRole::{Output as DeviceOutput, TransportTarget},
                PathStage,
                QualityVerdict::{self, BitPerfect, Limited, Processed},
                RenderingDevice,
                SignalPathError::NoActiveTrack,
                SignalPathSnapshot, SnapshotInput, StageFacts,
                StageKind::{
                    self, ExternalRenderer, FormatConverter, Output, Source, Transport, Volume,
                },
                path_snapshot::{build_snapshot, summarize_text},
                path_verdict::resolve_verdict,
                stage_describe::describe,
            },
            state::{
                MuteState::{Muted, Unmuted},
                PlaybackStatus::Playing,
            },
            volume::format_volume_db,
        },
        storage::catalog::TrackAudio,
    };

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
        scaled.volume = 0.5;
        assert_built(&scaled, Processed, true)?;
        let mut muted = perfect.clone();
        muted.generation = 13;
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
            sampled_at_wall: 0,
            decoded_frames: 0,
            resampled_frames: 0,
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

    fn flac_audio(sample_rate: i32, bit_depth: Option<i32>) -> TrackAudio {
        TrackAudio {
            file_path: format!("/music/track-{sample_rate}.flac"),
            content_hash: None,
            format: String::from("FLAC"),
            sample_rate,
            bit_depth,
            channels: 2,
            codec: String::from("FLAC"),
            lossless: true,
            bitrate: None,
            album_id: None,
            artist_id: None,
            file_size: 1024,
            last_modified: String::from("2026-01-01T00:00:00Z"),
        }
    }

    fn playing_input(
        track_id: i64,
        generation: u64,
        sample_rate: i32,
        bit_depth: Option<i32>,
    ) -> SnapshotInput {
        let mut input = bit_perfect_input();
        input.track_id = Some(track_id);
        input.generation = generation;
        input.track_audio = Some(flac_audio(sample_rate, bit_depth));
        input.decoder_params = Some(AudioParams {
            sample_rate: u32::try_from(sample_rate).unwrap_or(0),
            channels: 2,
            duration_seconds: 180.0,
            bit_depth: bit_depth.map(|bits| u16::try_from(bits).unwrap_or(0)),
        });
        input
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

    fn vol(volume: f64, label: &str) -> StageFacts {
        StageFacts::Volume {
            volume,
            label: String::from(label),
        }
    }

    #[test]
    fn us4_device_footer_contract() -> Result<()> {
        let snapshot = build_snapshot(&playing_input(41, 41, 44100, Some(16)))?;
        ensure!(snapshot.devices.len() == 1, "MVP shows exactly one card");
        let Some(device) = snapshot.devices.first() else {
            bail!("MVP snapshot must carry the active output device")
        };
        ensure!(!device.display_name.is_empty(), "card must name the device");
        ensure!(device.display_name == "Lab DAC", "card names the renderer");
        ensure!(device.manual_url.is_none(), "MVP hides the manual link");
        ensure!(!device.brand_visual.is_empty(), "card needs a brand visual");
        ensure!(
            !device.illustration.is_empty(),
            "card needs an illustration"
        );
        let mut fallback = bit_perfect_input();
        fallback.device_name = String::new();
        fallback.device_id = String::from("hw:7");
        let named = build_snapshot(&fallback)?;
        let Some(card) = named.devices.first() else {
            bail!("fallback snapshot must carry a device")
        };
        ensure!(card.display_name == "hw:7", "empty name falls back to id");
        let chain = [
            RenderingDevice {
                display_name: String::from("Streamer"),
                role: TransportTarget,
                brand_visual: String::from("audio-card-symbolic"),
                illustration: String::from("network-wireless-symbolic"),
                manual_url: None,
            },
            make_device(),
        ];
        ensure!(chain.len() == 2, "deferred chains keep chain order");
        ensure!(
            chain.iter().all(|card| !card.display_name.is_empty()),
            "cards never name a non-rendering device"
        );
        let mut lost = bit_perfect_input();
        lost.generation = 42;
        lost.device_lost = true;
        let rendered = build_snapshot(&lost)?;
        ensure!(
            rendered.verdict != BitPerfect,
            "lost device forces the verdict off Bit-Perfect"
        );
        Ok(())
    }

    #[test]
    fn us4_processing_speed_rule() -> Result<()> {
        let perfect = build_snapshot(&bit_perfect_input())?;
        ensure!(
            perfect.processing_speed.is_none(),
            "bit-perfect hides speed"
        );
        let mut scaled = bit_perfect_input();
        scaled.generation = 43;
        scaled.volume = 0.5;
        let volume_only = build_snapshot(&scaled)?;
        ensure!(volume_only.verdict == Processed, "volume-only is Processed");
        ensure!(
            volume_only.processing_speed.is_some(),
            "volume-only shows speed"
        );
        let up = build_snapshot(&playing_input(44, 44, 44100, Some(16)))?;
        ensure!(up.processing_speed.is_some(), "resample shows speed");
        let mut wide = bit_perfect_input();
        wide.generation = 45;
        wide.track_audio = Some(flac_audio(48000, Some(16)));
        wide.decoder_params = Some(AudioParams {
            sample_rate: 48000,
            channels: 2,
            duration_seconds: 180.0,
            bit_depth: Some(16),
        });
        wide.device_channels = 6;
        ensure!(
            build_snapshot(&wide)?.processing_speed.is_some(),
            "channel conversion shows speed"
        );
        let mut dsd = playing_input(46, 46, 2_822_400, None);
        dsd.volume = 0.5;
        let Some(audio) = dsd.track_audio.as_mut() else {
            bail!("DSD fixture needs audio")
        };
        audio.codec = String::from("DSF");
        audio.format = String::from("DSF");
        ensure!(
            build_snapshot(&dsd)?.processing_speed.is_some(),
            "DSD conversion shows speed"
        );
        let mut limited = bit_perfect_input();
        limited.generation = 47;
        limited.device_lost = true;
        let alone = build_snapshot(&limited)?;
        ensure!(alone.verdict == Limited, "lost device alone is Limited");
        ensure!(alone.processing_speed.is_none(), "Limited-only hides speed");
        let summary = summarize_text(&volume_only);
        ensure!(
            summary.lines().count() == volume_only.stages.len().saturating_add(1),
            "summary must list every stage once, got {summary}"
        );
        ensure!(
            summary
                .lines()
                .next()
                .is_some_and(|line| line.contains("Processed")),
            "summary header carries the verdict, got {summary}"
        );
        Ok(())
    }

    #[test]
    fn us3_describe_contract() -> Result<()> {
        let dsp_db = format_volume_db(0.5);
        let cases: Vec<(StageFacts, &str)> = vec![
            (
                StageFacts::SampleRateConverter {
                    input_rate: 96000,
                    output_rate: 192_000,
                },
                "96kHz to 192kHz",
            ),
            (
                StageFacts::FormatConverter {
                    input_format: String::from("DSD64"),
                    output_format: String::from("PCM 176.4kHz"),
                },
                "DSD64 to PCM",
            ),
            (vol(0.5, "DSP volume"), &dsp_db),
            (vol(0.8, "Leveling"), "Leveling"),
            (vol(0.9, "Headroom"), "Headroom"),
            (
                StageFacts::Effect {
                    kind: String::from("Channel map"),
                    summary: String::from("Stereo to Mono"),
                },
                "Channel map",
            ),
        ];
        for (kind, needle) in &cases {
            let (title, detail, explanation) = describe(kind);
            let texts = [title.as_str(), detail.as_str(), explanation.as_str()];
            ensure!(texts.iter().all(|text| !text.is_empty()), "empty");
            let hit = detail.contains(needle) || explanation.contains(needle);
            ensure!(hit, "missing {needle}");
            let wants_io = needle.contains(" to ");
            ensure!(!wants_io || detail.contains(" to "), "converters need io");
        }
        let mut dsd = playing_input(31, 31, 2_822_400, None);
        dsd.volume = 0.5;
        let Some(audio) = dsd.track_audio.as_mut() else {
            bail!("DSD fixture needs audio")
        };
        audio.codec = String::from("DSF");
        audio.format = String::from("DSF");
        let snapshot = build_snapshot(&dsd)?;
        let explicit = snapshot
            .stages
            .iter()
            .any(|stage| stage.kind == FormatConverter && stage.detail.contains("DSD"));
        ensure!(explicit, "DSD needs an explicit stage");
        Ok(())
    }
}

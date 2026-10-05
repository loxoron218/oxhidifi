//! Immutable signal path snapshot builder from cloned engine facts.
//!
//! Synchronous off-thread builder: the worker performs the async catalog
//! lookup and all `parking_lot` clones before calling [`build_snapshot`],
//! so no async I/O runs inside the builder or while holding a lock. Each
//! call produces one [`SignalPathSnapshot`] that the UI swaps atomically on
//! `generation` change; the audio hot path is never instrumented. Stage
//! construction lives in [`stage_build`](crate::playback::signal_path::stage_build).
//!
//! Tracing uses flat `generation`, `track_id`, `verdict`, and `stage_count`
//! fields on every build.

use tracing::info;

use crate::playback::signal_path::{
    PathStage, QualityVerdict,
    SignalPathError::{self, NoActiveTrack},
    SignalPathSnapshot, SnapshotInput,
    path_verdict::resolve_verdict,
    stage_build::{
        push_channel_stage, push_decoder_stage, push_dsd_stage, push_resample_stage,
        push_source_stage, push_volume_stage,
    },
    stage_output::{output_device, push_output_stage, push_transport_stage},
    stage_volume::{push_device_volume_stage, push_system_volume_stage},
};

/// Placeholder processing-speed multiple reported while real sampling lands.
///
/// Informational only: contracts assert presence (`Some`/`None`), never the
/// number. [`build_snapshot`] reports `Some` iff an in-app alteration is
/// active; the publisher worker smooths samples with its `SpeedEma` state.
const PLACEHOLDER_SPEED: f64 = 32.0;

/// Build one immutable snapshot from cloned engine/catalog state.
///
/// # Arguments
///
/// * `input` - Cloned track facts (`TrackAudio` via async `get_track`), decoder params
///   (`AudioParams`), pipeline facts (resampler in/out rates, channel counts), playback facts
///   (volume, mute, output mode, status), device/hardware volume facts (ALSA read-back when
///   available, else the engine-set slider value in `BitPerfect` mode), system/application volume
///   facts (`MixerSample` level, mute, and source — unreadable levels render as `unknown`), and
///   output facts (device id/name/rate/mode). Provider auth facts are optional input (`None` in
///   MVP, so no Authentication stage is emitted).
///
/// # Returns
///
/// * `Result<SignalPathSnapshot, SignalPathError>` - Ready-to-render snapshot, or a typed build
///   error (`NoActiveTrack` maps to the empty state, not a failure for the UI).
///
/// # Errors
///
/// Returns [`SignalPathError::NoActiveTrack`] when `track_id` is `None`.
pub fn build_snapshot(input: &SnapshotInput) -> Result<SignalPathSnapshot, SignalPathError> {
    let track_id = input.track_id.ok_or(NoActiveTrack)?;
    let mut stages: Vec<PathStage> = Vec::new();
    push_source_stage(&mut stages, input);
    push_decoder_stage(&mut stages, &input.decoder_params);
    let dsd_emitted = push_dsd_stage(&mut stages, input);
    let resample_emitted = push_resample_stage(&mut stages, input);
    let channels_emitted = push_channel_stage(&mut stages, input);
    let dsp_volume_emitted = push_volume_stage(&mut stages, input);
    push_device_volume_stage(&mut stages, input);
    push_system_volume_stage(&mut stages, input);
    push_transport_stage(&mut stages, input);
    push_output_stage(&mut stages, input);
    let verdicts: Vec<QualityVerdict> = stages.iter().map(|stage| stage.verdict).collect();
    let verdict = resolve_verdict(&verdicts);
    let alteration_active =
        dsd_emitted || resample_emitted || channels_emitted || dsp_volume_emitted;
    let processing_speed = alteration_active.then_some(PLACEHOLDER_SPEED);
    let zone_name = if input.zone_name.trim().is_empty() {
        output_device(input).display_name
    } else {
        input.zone_name.clone()
    };
    let snapshot = SignalPathSnapshot {
        generation: input.generation,
        track_id: Some(track_id),
        zone_name,
        verdict,
        stages,
        devices: vec![output_device(input)],
        processing_speed,
        playback_status: input.status,
    };
    info!(
        generation = snapshot.generation,
        track_id = track_id,
        verdict = snapshot.verdict.label(),
        stage_count = snapshot.stages.len(),
        "Built signal path snapshot",
    );
    Ok(snapshot)
}

/// Render the overflow-menu "Copy path summary" plain-text summary.
///
/// # Arguments
///
/// * `snapshot` - Snapshot to summarize.
///
/// # Returns
///
/// * `String` - Multi-line `Title — detail` chain with header verdict.
#[must_use]
pub fn summarize_text(snapshot: &SignalPathSnapshot) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "{} — {}",
        snapshot.verdict.label(),
        snapshot.zone_name
    ));
    for stage in &snapshot.stages {
        lines.push(format!("{} — {}", stage.title, stage.detail));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail, ensure};

    use crate::{
        playback::{
            alsa_mixer::SystemMixerSource::Unknown,
            decoder::AudioParams,
            devices::OutputMode::{BitPerfect, Resampled},
            signal_path::{
                SignalPathError::NoActiveTrack,
                SnapshotInput,
                StageKind::{Decoder, Source},
                path_snapshot::{build_snapshot, summarize_text},
            },
            state::{
                MuteState::{Muted, Unmuted},
                PlaybackStatus::Playing,
            },
        },
        storage::catalog::TrackAudio,
    };

    fn catalog_input() -> SnapshotInput {
        SnapshotInput {
            track_id: Some(21),
            generation: 7,
            track_audio: Some(TrackAudio {
                file_path: String::from("/music/lab-track.flac"),
                content_hash: None,
                format: String::from("FLAC"),
                sample_rate: 44100,
                bit_depth: Some(16),
                channels: 2,
                codec: String::from("FLAC"),
                lossless: true,
                bitrate: None,
                album_id: None,
                artist_id: None,
                file_size: 4096,
                last_modified: String::from("2026-03-01T00:00:00Z"),
            }),
            decoder_params: Some(AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 240.0,
                bit_depth: Some(16),
            }),
            resampler_in_rate: None,
            resampler_out_rate: None,
            resampler_channels: None,
            volume: 1.0,
            muted: Unmuted,
            device_volume: None,
            device_muted: false,
            system_volume: None,
            system_muted: false,
            system_source: Unknown,
            output_mode: BitPerfect,
            status: Playing,
            device_id: String::from("hw:1"),
            device_name: String::from("Lab DAC"),
            device_sample_rate: 44100,
            device_channels: 2,
            device_lost: false,
            zone_name: String::from("Lab DAC"),
            auth: None,
            sampled_at_wall: 0,
            decoded_frames: 0,
            resampled_frames: 0,
        }
    }

    #[test]
    fn empty_track_maps_to_no_active_track() -> Result<()> {
        let mut input = catalog_input();
        input.track_id = None;
        let Err(err) = build_snapshot(&input) else {
            bail!("expected NoActiveTrack for a missing track")
        };
        ensure!(
            matches!(err, NoActiveTrack),
            "missing track must map to NoActiveTrack, got {err:?}"
        );
        Ok(())
    }

    #[test]
    fn source_stage_uses_catalog_facts() -> Result<()> {
        let snapshot = build_snapshot(&catalog_input())?;
        let Some(first) = snapshot.stages.first() else {
            bail!("snapshot must contain stages")
        };
        ensure!(first.kind == Source, "first stage must be Source");
        ensure!(
            first.detail.contains("FLAC"),
            "detail was: {}",
            first.detail
        );
        ensure!(
            first.detail.contains("44.1kHz"),
            "detail was: {}",
            first.detail
        );
        ensure!(
            first.detail.contains("16-bit"),
            "detail was: {}",
            first.detail
        );
        ensure!(
            !first.explanation.is_empty(),
            "explanation must not be empty"
        );
        Ok(())
    }

    #[test]
    fn decoder_facts_add_decoder_stage() -> Result<()> {
        let snapshot = build_snapshot(&catalog_input())?;
        let decoder = snapshot.stages.iter().find(|stage| stage.kind == Decoder);
        ensure!(decoder.is_some(), "decoder facts must emit a Decoder stage");
        let Some(stage) = decoder else {
            bail!("decoder stage must exist")
        };
        ensure!(
            stage.detail.contains("44.1kHz"),
            "detail was: {}",
            stage.detail
        );
        Ok(())
    }

    #[test]
    fn dsp_volume_icon_tracks_level_and_mute() -> Result<()> {
        let mut input = catalog_input();
        input.output_mode = Resampled;
        input.volume = 0.5;
        let snapshot = build_snapshot(&input)?;
        let Some(stage) = snapshot.stages.iter().find(|stage| stage.title == "Volume") else {
            bail!("resampled attenuation must emit DSP Volume")
        };
        ensure!(
            stage.badge_icon == "audio-volume-medium-symbolic",
            "50 % DSP must render medium, got {}",
            stage.badge_icon
        );
        let mut muted = catalog_input();
        muted.output_mode = Resampled;
        muted.volume = 0.5;
        muted.muted = Muted;
        let snapshot = build_snapshot(&muted)?;
        let Some(stage) = snapshot.stages.iter().find(|stage| stage.title == "Volume") else {
            bail!("muted DSP must emit Volume")
        };
        ensure!(
            stage.badge_icon == "audio-volume-muted-symbolic",
            "muted DSP must render muted, got {}",
            stage.badge_icon
        );
        Ok(())
    }

    #[test]
    fn summarize_lists_every_stage_once_in_order() -> Result<()> {
        let snapshot = build_snapshot(&catalog_input())?;
        let summary = summarize_text(&snapshot);
        let lines: Vec<&str> = summary.lines().collect();
        ensure!(
            lines.len() == snapshot.stages.len().saturating_add(1),
            "summary must list the header plus every stage once, got {summary}"
        );
        ensure!(
            lines
                .first()
                .is_some_and(|line| line.contains("Bit-Perfect")),
            "first line must carry the header verdict, got {summary}"
        );
        for (stage, line) in snapshot.stages.iter().zip(lines.iter().skip(1)) {
            ensure!(
                line.contains(stage.title.as_str()),
                "line must contain its stage title, got {summary}"
            );
        }
        Ok(())
    }
}

//! Signal path stage constructors from cloned engine facts.
//!
//! Pure helpers behind
//! [`build_snapshot`](crate::playback::signal_path::path_snapshot::build_snapshot): one `push_*`
//! function per conversion and volume stage family. Stages run source-at-top
//! to output-at-bottom; every altering stage is emitted explicitly so no
//! conversion is ever silent. Transport, output, and device stages live in
//! [`stage_output`](crate::playback::signal_path::stage_output).

use crate::playback::{
    decoder::AudioParams,
    signal_path::{
        PathStage,
        QualityVerdict::{self, BitPerfect, Limited, Processed},
        SnapshotInput,
        StageKind::{self, Decoder, FormatConverter, SampleRateConverter, Source, Volume},
    },
    state::MuteState::Muted,
    volume::format_volume_db,
};

/// Convert a catalog integer to a positive `u32` rate, if representable.
fn catalog_rate(value: i32) -> Option<u32> {
    let rate = u32::try_from(value).unwrap_or(0);
    (rate > 0).then_some(rate)
}

/// Convert a catalog integer to a positive `u16` count, if representable.
fn catalog_count(value: i32) -> Option<u16> {
    let count = u16::try_from(value).unwrap_or(0);
    (count > 0).then_some(count)
}

/// Format a sample rate in Hz as a compact label such as `44.1kHz`.
fn format_rate(hz: u32) -> String {
    let remainder = hz.checked_rem(1000).unwrap_or(0);
    if remainder == 0 {
        let whole = hz.checked_div(1000).unwrap_or(0);
        format!("{whole}kHz")
    } else {
        let khz = f64::from(hz) / 1000.0;
        format!("{khz:.1}kHz")
    }
}

/// Format a channel count as `Mono`, `Stereo`, or `{n} channels`.
fn format_channels(channels: u16) -> String {
    match channels {
        1 => String::from("Mono"),
        2 => String::from("Stereo"),
        count => format!("{count} channels"),
    }
}

/// Return `value` when non-empty, otherwise the `unknown` placeholder.
fn known_or_unknown(value: &str) -> &str {
    if value.trim().is_empty() {
        "unknown"
    } else {
        value
    }
}

/// Check whether a codec/format label identifies a DSD-family stream.
fn is_dsd_label(label: &str) -> bool {
    let lower = label.to_lowercase();
    lower.contains("dsd") || lower.contains("dsf") || lower.contains("dff") || lower.contains("dop")
}

/// Check whether DSP-volume scaling is active, including volume-only paths.
fn is_volume_active(input: &SnapshotInput) -> bool {
    input.volume < 1.0 || input.muted == Muted
}

/// Resolve the live source sample rate, preferring decoder truth.
fn live_rate_hz(input: &SnapshotInput) -> Option<u32> {
    if let Some(params) = &input.decoder_params {
        return Some(params.sample_rate);
    }
    if let Some(audio) = &input.track_audio {
        return catalog_rate(audio.sample_rate);
    }
    None
}

/// Append one stage, assigning the next `position` in chain order.
pub fn push_stage(
    stages: &mut Vec<PathStage>,
    kind: StageKind,
    title: String,
    detail: String,
    explanation: String,
    verdict: QualityVerdict,
    badge_icon: &'static str,
) {
    let position = u32::try_from(stages.len()).unwrap_or(u32::MAX);
    stages.push(PathStage {
        position,
        kind,
        title,
        detail,
        explanation,
        verdict,
        badge_icon,
    });
}

/// Append the always-present Source stage from catalog/decoder facts.
pub fn push_source_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) {
    let (codec, rate_hz, depth_bits, channels) = match (&input.track_audio, &input.decoder_params) {
        (Some(audio), _) => (
            known_or_unknown(&audio.codec).to_string(),
            catalog_rate(audio.sample_rate),
            catalog_count(audio.bit_depth.unwrap_or(0)),
            catalog_count(audio.channels),
        ),
        (None, Some(params)) => (
            String::from("unknown"),
            Some(params.sample_rate),
            params.bit_depth,
            Some(params.channels),
        ),
        (None, None) => (String::from("unknown"), None, None, None),
    };
    let rate_text = rate_hz.map_or_else(|| String::from("unknown"), format_rate);
    let depth_text =
        depth_bits.map_or_else(|| String::from("unknown"), |depth| format!("{depth}-bit"));
    let channels_text = channels.map_or_else(|| String::from("unknown"), format_channels);
    let detail = format!("{codec} {rate_text} {depth_text} {channels_text}");
    let explanation = format!(
        "The original {codec} audio at {rate_text}, {depth_text}, {channels_text}. Later stages \
         preserve it exactly unless they report a conversion."
    );
    push_stage(
        stages,
        Source,
        String::from("Source"),
        detail,
        explanation,
        BitPerfect,
        "audio-x-generic-symbolic",
    );
}

/// Append the Decoder stage when live decoder facts are available.
pub fn push_decoder_stage(stages: &mut Vec<PathStage>, decoder: &Option<AudioParams>) {
    let Some(params) = decoder else {
        return;
    };
    let detail = format!(
        "PCM {} {}",
        format_rate(params.sample_rate),
        format_channels(params.channels)
    );
    push_stage(
        stages,
        Decoder,
        String::from("Decoder"),
        detail,
        String::from(
            "Decodes the compressed stream into raw PCM frames without changing the audio content.",
        ),
        BitPerfect,
        "application-x-executable-symbolic",
    );
}

/// Append the explicit DSD-to-PCM stage when DSD meets volume or DSP.
///
/// Silent omission is a contract violation, so any DSD source undergoing
/// volume scaling or resampling always produces this stage.
pub fn push_dsd_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) -> bool {
    let dsd_source = input
        .track_audio
        .as_ref()
        .is_some_and(|audio| is_dsd_label(&audio.codec) || is_dsd_label(&audio.format));
    let dsp_active = is_volume_active(input)
        || input.resampler_in_rate.is_some()
        || input.resampler_out_rate.is_some();
    if !(dsd_source && dsp_active) {
        return false;
    }
    let out_rate = input.resampler_out_rate.unwrap_or(input.device_sample_rate);
    let detail = format!("DSD to PCM {}", format_rate(out_rate));
    let explanation = format!(
        "Converts the 1-bit DSD stream to PCM at {} so volume and mixing can apply. This \
         intentionally alters the bit stream.",
        format_rate(out_rate)
    );
    push_stage(
        stages,
        FormatConverter,
        String::from("DSD to PCM Conversion"),
        detail,
        explanation,
        Processed,
        "media-optical-cd-audio-symbolic",
    );
    true
}

/// Detect an unreported rate mismatch between source and device.
fn defensive_resample(input: &SnapshotInput) -> Option<(u32, u32)> {
    if input.device_sample_rate == 0 {
        return None;
    }
    live_rate_hz(input)
        .filter(|rate| {
            *rate != input.device_sample_rate
                && input.resampler_in_rate.is_none()
                && input.resampler_out_rate.is_none()
        })
        .map(|rate| (rate, input.device_sample_rate))
}

/// Append the sample-rate converter stage when conversion is active.
///
/// Covers both explicit resampler facts and a defensive rate mismatch
/// between the live source rate and the device rate, so no conversion is
/// ever silent. Forced downsampling reports `Limited`.
pub fn push_resample_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) -> bool {
    let conversion = match (input.resampler_in_rate, input.resampler_out_rate) {
        (Some(ins), Some(out)) if ins != out => Some((ins, out)),
        _ => defensive_resample(input),
    };
    let Some((ins, out)) = conversion else {
        return false;
    };
    let detail = format!("{} to {}", format_rate(ins), format_rate(out));
    let verdict = if out < ins { Limited } else { Processed };
    let explanation = format!(
        "Converts the sample rate from {} to {} so the stream matches the output device. This \
         intentionally alters samples.",
        format_rate(ins),
        format_rate(out)
    );
    push_stage(
        stages,
        SampleRateConverter,
        String::from("Sample Rate Conversion"),
        detail,
        explanation,
        verdict,
        "audio-card-symbolic",
    );
    true
}

/// Append the channel conversion stage when source and device differ.
pub fn push_channel_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) -> bool {
    let decoded = input.decoder_params.map(|params| params.channels);
    let cataloged = input
        .track_audio
        .as_ref()
        .and_then(|audio| catalog_count(audio.channels));
    let src = decoded.or(cataloged);
    let Some(src) = src else {
        return false;
    };
    if src == input.device_channels || input.device_channels == 0 {
        return false;
    }
    let detail = format!(
        "{} to {}",
        format_channels(src),
        format_channels(input.device_channels)
    );
    let explanation = format!(
        "Remaps channels from {} to {} to fit the output device. This intentionally alters the \
         stream.",
        format_channels(src),
        format_channels(input.device_channels)
    );
    push_stage(
        stages,
        FormatConverter,
        String::from("Channel Conversion"),
        detail,
        explanation,
        Processed,
        "audio-speakers-symbolic",
    );
    true
}

/// Append the single DSP-volume stage iff software scaling is active.
pub fn push_volume_stage(stages: &mut Vec<PathStage>, input: &SnapshotInput) -> bool {
    if !is_volume_active(input) {
        return false;
    }
    let detail = if input.muted == Muted {
        format!("Muted ({})", format_volume_db(input.volume))
    } else {
        format_volume_db(input.volume)
    };
    let explanation = format!(
        "Applies the DSP volume control ({detail}) by scaling samples in software. Lowering \
         volume here intentionally alters the bit stream."
    );
    push_stage(
        stages,
        Volume,
        String::from("Volume"),
        detail,
        explanation,
        Processed,
        "audio-volume-high-symbolic",
    );
    true
}

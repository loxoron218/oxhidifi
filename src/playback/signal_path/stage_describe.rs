//! Pure stage descriptions: titles, details, and explanations.
//!
//! [`describe`] is total over [`StageFacts`]: every variant maps to a
//! non-empty triple; converters carry input, volumes dB.

use crate::playback::{
    signal_path::{
        StageFacts,
        stage_build::{format_channels, format_rate, is_dsd_label},
    },
    volume::format_volume_db,
};

/// Return `value` trimmed when non-empty, otherwise the `fallback` placeholder.
fn known_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        String::from(fallback)
    } else {
        String::from(value.trim())
    }
}

/// Check whether a format label names channel geometry.
fn is_channel_label(label: &str) -> bool {
    let lower = label.to_lowercase();
    lower.contains("mono")
        || lower.contains("stereo")
        || lower.contains("channel")
        || lower.contains("5.1")
}

/// Describe one stage kind with input-to-output facts.
///
/// Explanation is one or two sentences plus values where applicable, never
/// empty. Effect arms stay vocabulary-only in MVP.
///
/// # Arguments
///
/// * `kind` - Stage kind plus its numeric/string parameters.
///
/// # Returns
///
/// * `(String, String, String)` - (title, detail, plain-language explanation).
#[must_use]
pub fn describe(kind: &StageFacts) -> (String, String, String) {
    match kind {
        StageFacts::Source {
            origin,
            codec,
            sample_rate,
            bit_depth,
            channels,
        } => describe_source(origin, codec, *sample_rate, *bit_depth, *channels),
        StageFacts::Authentication { provider, verified } => {
            describe_authentication(provider, *verified)
        }
        StageFacts::Decoder {
            codec,
            sample_rate,
            channels,
        } => describe_decoder(codec, *sample_rate, *channels),
        StageFacts::BitDepthConverter {
            input_bits,
            output_bits,
        } => describe_bit_depth(*input_bits, *output_bits),
        StageFacts::SampleRateConverter {
            input_rate,
            output_rate,
        } => describe_sample_rate(*input_rate, *output_rate),
        StageFacts::FormatConverter {
            input_format,
            output_format,
        } => describe_format(input_format, output_format),
        StageFacts::Volume { volume, label } => describe_volume(*volume, label),
        StageFacts::Effect { kind, summary } => describe_effect(kind, summary),
        StageFacts::Transport { mode, device_name } => describe_transport(mode, device_name),
        StageFacts::Output { destination, mode } => describe_output(destination, mode),
        StageFacts::ExternalRenderer {
            title,
            filter,
            modulator,
        } => describe_renderer(title, filter.as_ref(), modulator.as_ref()),
    }
}
/// Describe the source origin and format facts.
fn describe_source(
    origin: &str,
    codec: &str,
    sample_rate: Option<u32>,
    bit_depth: Option<u16>,
    channels: Option<u16>,
) -> (String, String, String) {
    let origin_text = known_or(origin, "unknown source");
    let codec_text = known_or(codec, "unknown");
    let rate_text = sample_rate.map_or_else(|| String::from("unknown"), format_rate);
    let depth_text =
        bit_depth.map_or_else(|| String::from("unknown"), |depth| format!("{depth}-bit"));
    let channels_text = channels.map_or_else(|| String::from("unknown"), format_channels);
    let title = String::from("Source");
    let detail = format!("{codec_text} {rate_text} {depth_text} {channels_text}");
    let explanation = format!(
        "The original {codec_text} audio at {rate_text}, {depth_text}, {channels_text} from \
         {origin_text}. Later stages preserve it unless they report a conversion."
    );
    (title, detail, explanation)
}
/// Describe provider authentication or verification.
fn describe_authentication(provider: &str, verified: bool) -> (String, String, String) {
    let provider_text = known_or(provider, "unknown provider");
    let state = if verified { "verified" } else { "unverified" };
    let title = String::from("Authentication");
    let detail = format!("{provider_text} {state}");
    let explanation = format!(
        "Verifies the {provider_text} stream is authentic ({state}). A verified stream preserves \
         the provider master."
    );
    (title, detail, explanation)
}

/// Describe codec decoding into PCM frames.
fn describe_decoder(codec: &str, sample_rate: u32, channels: u16) -> (String, String, String) {
    let codec_text = known_or(codec, "unknown");
    let rate_text = format_rate(sample_rate);
    let channels_text = format_channels(channels);
    let title = String::from("Decoder");
    let detail = format!("{codec_text} to PCM {rate_text} {channels_text}");
    let explanation = format!(
        "Decodes {codec_text} into raw PCM at {rate_text}, {channels_text} without changing the \
         audio content."
    );
    (title, detail, explanation)
}

/// Describe bit-depth conversion with input-to-output wording.
fn describe_bit_depth(input_bits: u16, output_bits: u16) -> (String, String, String) {
    let title = format!("Bit Depth Conversion {input_bits}bit to {output_bits}bit");
    let detail = format!("{input_bits}bit to {output_bits}bit");
    let explanation = format!(
        "Converts bit depth from {input_bits}bit to {output_bits}bit to fit the processing path. \
         This intentionally alters samples."
    );
    (title, detail, explanation)
}

/// Describe sample-rate conversion with input-to-output wording.
fn describe_sample_rate(input_rate: u32, output_rate: u32) -> (String, String, String) {
    let input_text = format_rate(input_rate);
    let output_text = format_rate(output_rate);
    let title = String::from("Sample Rate Conversion");
    let detail = format!("{input_text} to {output_text}");
    let explanation = format!(
        "Converts {input_text} to {output_text} to match the output device. This intentionally \
         alters samples."
    );
    (title, detail, explanation)
}

/// Describe format conversion such as DSD-to-PCM or channel remapping.
fn describe_format(input_format: &str, output_format: &str) -> (String, String, String) {
    let input_text = known_or(input_format, "unknown");
    let output_text = known_or(output_format, "unknown");
    if is_dsd_label(&input_text) {
        let title = String::from("DSD to PCM Conversion");
        let detail = format!("{input_text} to {output_text}");
        let explanation = format!(
            "Converts the 1-bit DSD stream ({input_text}) to PCM ({output_text}) so volume and \
             mixing can apply. This intentionally alters the bit stream."
        );
        (title, detail, explanation)
    } else if is_channel_label(&input_text) || is_channel_label(&output_text) {
        let title = String::from("Channel Conversion");
        let detail = format!("{input_text} to {output_text}");
        let explanation = format!(
            "Remaps channels from {input_text} to {output_text} to fit the output device. This \
             intentionally alters the stream."
        );
        (title, detail, explanation)
    } else {
        let title = String::from("Format Conversion");
        let detail = format!("{input_text} to {output_text}");
        let explanation = format!(
            "Converts the stream format from {input_text} to {output_text} to fit the output \
             path. This intentionally alters samples."
        );
        (title, detail, explanation)
    }
}

/// Describe volume handling with its decibel value.
fn describe_volume(volume: f64, label: &str) -> (String, String, String) {
    let label_text = known_or(label, "Volume");
    let db = format_volume_db(volume);
    let title = label_text.clone();
    let detail = db.clone();
    let explanation = format!(
        "Applies {label_text} ({db}) by scaling samples. Lowering volume here intentionally \
         alters the bit stream."
    );
    (title, detail, explanation)
}

/// Describe an effect with its setting summary.
fn describe_effect(kind: &str, summary: &str) -> (String, String, String) {
    let kind_text = known_or(kind, "Effect");
    let summary_text = known_or(summary, "Effect settings");
    let title = kind_text.clone();
    let detail = summary_text.clone();
    let explanation = format!(
        "Applies {kind_text} ({summary_text}) to shape the sound. This intentionally alters \
         samples."
    );
    (title, detail, explanation)
}

/// Describe delivery transport with Linux output-mode wording.
fn describe_transport(mode: &str, device_name: &str) -> (String, String, String) {
    let mode_text = known_or(mode, "unknown transport");
    let device_text = known_or(device_name, "unknown device");
    let lower = mode_text.to_lowercase();
    let title = if lower.contains("shared") || lower.contains("mixer") {
        String::from("ALSA Shared Output")
    } else if lower.contains("usb") {
        String::from("USB Output")
    } else if lower.contains("direct") || lower.contains("exclusive") {
        String::from("ALSA Direct Output")
    } else if mode_text == "unknown transport" {
        String::from("Transport")
    } else {
        mode_text.clone()
    };
    let detail = format!("{device_text} ({mode_text})");
    let explanation = format!(
        "Delivers to {device_text} via {mode_text}. Shared mixers can alter the stream; direct \
         and USB paths preserve it."
    );
    (title, detail, explanation)
}

/// Describe the final destination device output.
fn describe_output(destination: &str, mode: &str) -> (String, String, String) {
    let destination_text = known_or(destination, "unknown");
    let mode_text = mode.trim().to_string();
    let title = String::from("Output");
    let detail = if mode_text.is_empty() {
        destination_text.clone()
    } else {
        format!("{destination_text} ({mode_text})")
    };
    let explanation = format!(
        "Renders the final stream on {destination_text}. The last step before the speakers or \
         headphones."
    );
    (title, detail, explanation)
}

/// Describe hand-off beyond the application to an external renderer.
fn describe_renderer(
    title: &str,
    filter: Option<&String>,
    modulator: Option<&String>,
) -> (String, String, String) {
    let title_text = known_or(title, "External Renderer");
    let detail = title_text.clone();
    let extra = match (filter, modulator) {
        (Some(filter_text), Some(modulator_text)) => {
            format!(" (filter {filter_text}, modulator {modulator_text})")
        }
        (Some(filter_text), None) => format!(" (filter {filter_text})"),
        (None, Some(modulator_text)) => format!(" (modulator {modulator_text})"),
        (None, None) => String::new(),
    };
    let explanation = format!(
        "Hands the stream off to {title_text} beyond the application{extra}. No further in-app \
         processing applies here."
    );
    (title_text, detail, explanation)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::playback::{
        signal_path::{StageFacts, stage_describe::describe},
        volume::format_volume_db,
    };

    /// Contract facts with expected wording needles for the table test.
    fn describe_cases(dsp_db: &str) -> Vec<(StageFacts, &str)> {
        vec![
            (
                StageFacts::Source {
                    origin: String::from("File"),
                    codec: String::from("FLAC"),
                    sample_rate: Some(44100),
                    bit_depth: Some(16),
                    channels: Some(2),
                },
                "FLAC",
            ),
            (
                StageFacts::Authentication {
                    provider: String::from("Qobuz"),
                    verified: true,
                },
                "Qobuz",
            ),
            (
                StageFacts::Decoder {
                    codec: String::from("FLAC"),
                    sample_rate: 44100,
                    channels: 2,
                },
                "PCM",
            ),
            (
                StageFacts::BitDepthConverter {
                    input_bits: 24,
                    output_bits: 64,
                },
                "24bit to 64bit",
            ),
            (
                StageFacts::Volume {
                    volume: 0.5,
                    label: String::from("DSP volume"),
                },
                dsp_db,
            ),
            (
                StageFacts::Effect {
                    kind: String::from("Equalizer"),
                    summary: String::from("4 bands"),
                },
                "Equalizer",
            ),
            (
                StageFacts::Effect {
                    kind: String::from("Channel map"),
                    summary: String::from("Stereo to Mono"),
                },
                "Channel map",
            ),
            (
                StageFacts::Transport {
                    mode: String::from("ALSA direct exclusive"),
                    device_name: String::from("DAC"),
                },
                "ALSA",
            ),
            (
                StageFacts::Output {
                    destination: String::from("Speakers"),
                    mode: String::from("Direct"),
                },
                "Speakers",
            ),
            (
                StageFacts::ExternalRenderer {
                    title: String::from("HQPlayer"),
                    filter: None,
                    modulator: None,
                },
                "HQPlayer",
            ),
            (
                StageFacts::Source {
                    origin: String::new(),
                    codec: String::new(),
                    sample_rate: None,
                    bit_depth: None,
                    channels: None,
                },
                "unknown",
            ),
        ]
    }

    #[test]
    fn describe_covers_every_family_with_io_and_db() -> Result<()> {
        let dsp_db = format_volume_db(0.5);
        let cases = describe_cases(&dsp_db);
        for (kind, needle) in &cases {
            let (title, detail, explanation) = describe(kind);
            let texts = [title.as_str(), detail.as_str(), explanation.as_str()];
            ensure!(texts.iter().all(|text| !text.is_empty()), "empty");
            let hit = detail.contains(needle) || explanation.contains(needle);
            ensure!(hit, "missing {needle}");
            ensure!(
                !needle.contains(" to ") || detail.contains(" to "),
                "converters need io"
            );
            let renderer = matches!(kind, StageFacts::ExternalRenderer { .. });
            ensure!(!renderer || title == detail, "renderer stays title-only");
        }
        ensure!(cases.len() == 11, "every family covered");
        Ok(())
    }
}

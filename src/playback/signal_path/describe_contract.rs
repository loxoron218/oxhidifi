//! Shared describe contract check for stage wording.
//!
//! Single owner for the title/detail/explanation assertions used by the unit
//! test in [`stage_describe`](crate::playback::signal_path::stage_describe)
//! and the integration test target so copy-paste clones stay absent.

use crate::playback::{
    alsa_mixer::SystemMixerSource::{AlsaShared, Unknown},
    signal_path::{
        StageFacts::{
            self, Authentication, BitDepthConverter, Decoder, Effect, ExternalRenderer, Output,
            Source, SystemVolume, Transport, Volume,
        },
        stage_describe::describe,
    },
    volume::format_volume_db,
};

/// Check one describe contract pair for non-empty text and expected wording.
///
/// Verifies the title, detail, and explanation are non-empty, the detail or
/// explanation carries `needle`, converters keep input-to-output wording, and
/// external renderers stay title-only.
///
/// # Arguments
///
/// * `kind` - Stage facts to describe.
/// * `needle` - Expected substring in the detail or explanation.
///
/// # Returns
///
/// * `()` - Contract holds for this pair.
///
/// # Errors
///
/// Returns an error string when any text is empty, the needle is missing, a
/// converter drops input-to-output wording, or a renderer title diverges.
pub fn check_describe_pair(kind: &StageFacts, needle: &str) -> Result<(), String> {
    let (title, detail, explanation) = describe(kind);
    let texts = [title.as_str(), detail.as_str(), explanation.as_str()];
    if !texts.iter().all(|text| !text.is_empty()) {
        return Err(String::from("empty"));
    }
    let hit = detail.contains(needle) || explanation.contains(needle);
    if !hit {
        return Err(format!("missing {needle}"));
    }
    if needle.contains(" to ") && !detail.contains(" to ") {
        return Err(String::from("converters need io"));
    }
    let renderer = matches!(kind, ExternalRenderer { .. });
    if renderer && title != detail {
        return Err(String::from("renderer stays title-only"));
    }
    Ok(())
}

/// Shared describe cases covering every stage family with wording needles.
///
/// Single owner for the core matrix asserted by the unit test in
/// [`stage_describe`](crate::playback::signal_path::stage_describe) so
/// copy-paste clones stay absent.
///
/// # Returns
///
/// * `Vec<(StageFacts, String)>` - Facts plus the expected detail needle.
#[must_use]
pub fn core_describe_cases() -> Vec<(StageFacts, String)> {
    let dsp_db = format_volume_db(0.5);
    vec![
        (
            Source {
                origin: String::from("File"),
                codec: String::from("FLAC"),
                sample_rate: Some(44100),
                bit_depth: Some(16),
                channels: Some(2),
            },
            String::from("FLAC"),
        ),
        (
            Authentication {
                provider: String::from("Qobuz"),
                verified: true,
            },
            String::from("Qobuz"),
        ),
        (
            Decoder {
                codec: String::from("FLAC"),
                sample_rate: 44100,
                channels: 2,
            },
            String::from("PCM"),
        ),
        (
            BitDepthConverter {
                input_bits: 24,
                output_bits: 64,
            },
            String::from("24bit to 64bit"),
        ),
        (
            Volume {
                volume: 0.5,
                label: String::from("DSP volume"),
            },
            dsp_db,
        ),
        (
            Effect {
                kind: String::from("Equalizer"),
                summary: String::from("4 bands"),
            },
            String::from("Equalizer"),
        ),
        (
            Effect {
                kind: String::from("Channel map"),
                summary: String::from("Stereo to Mono"),
            },
            String::from("Channel map"),
        ),
        (
            Transport {
                mode: String::from("ALSA direct exclusive"),
                device_name: String::from("DAC"),
            },
            String::from("ALSA"),
        ),
        (
            Output {
                destination: String::from("Speakers"),
                mode: String::from("Direct"),
            },
            String::from("Speakers"),
        ),
        (
            ExternalRenderer {
                title: String::from("HQPlayer"),
                filter: None,
                modulator: None,
            },
            String::from("HQPlayer"),
        ),
        (
            Source {
                origin: String::new(),
                codec: String::new(),
                sample_rate: None,
                bit_depth: None,
                channels: None,
            },
            String::from("unknown"),
        ),
    ]
}

/// Needle case for the `Device Volume` describe arm (ALSA hardware plus dB).
///
/// # Arguments
///
/// * `volume` - Linear slider value from 0.0 to 1.0.
///
/// # Returns
///
/// * `(StageFacts, String)` - Facts plus the expected dB detail.
#[must_use]
pub fn device_volume_needle(volume: f64) -> (StageFacts, String) {
    let detail = format_volume_db(volume);
    (
        Volume {
            volume,
            label: String::from("Device Volume"),
        },
        detail,
    )
}

/// Needle case for the `System Volume` describe arm (OS mixer plus dB or `unknown`).
///
/// # Arguments
///
/// * `volume` - OS-mixer level (`None` renders `unknown`).
/// * `muted` - Whether the OS-mixer mute switch is engaged.
///
/// # Returns
///
/// * `(StageFacts, String)` - Facts plus the expected detail needle.
pub fn system_volume_needle(volume: Option<f64>, muted: bool) -> (StageFacts, String) {
    let source = if volume.is_some() {
        AlsaShared
    } else {
        Unknown
    };
    let needle = volume.map_or_else(|| String::from("unknown"), format_volume_db);
    (
        SystemVolume {
            volume,
            muted,
            source,
        },
        needle,
    )
}

//! Output helpers for bit-perfect detection and ALSA volume.
//!
//! Extracted from `output.rs` to keep each file under 400 lines.

use std::{
    process::Command,
    sync::LazyLock,
    time::{Duration, Instant},
};

use {
    cpal::SampleFormat::{self, F32, I16, I32, U16, U32},
    parking_lot::Mutex,
    tracing::{info, warn},
};

use crate::playback::{
    alsa_mixer::{AlsaVolumeControl, MixerSample, SystemMixerSource::PipeWirePulse},
    devices::alsa_card_name,
};

/// How long a failed card stays suppressed before the next open attempt.
const MIXER_RETRY_INTERVAL: Duration = Duration::from_secs(30);

/// Last mixer open failure per card name, shared by every opener.
static MIXER_FAILURES: LazyLock<Mutex<Vec<(String, Instant)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Check whether the device supports bit-perfect playback at the given rate and depth.
///
/// Bit-depth checking maps `bit_depth` to the device's [`SampleFormat`]:
/// 16-bit → I16/U16, 24-bit → I32 (24-bit is carried in 32-bit containers),
/// 32-bit → I32/F32/U32.
#[must_use]
pub const fn supports_native(
    sample_rate: u32,
    bit_depth: u16,
    device_rate: u32,
    sample_format: SampleFormat,
) -> bool {
    if device_rate != sample_rate {
        return false;
    }
    if bit_depth == 0 {
        return true;
    }
    match sample_format {
        F32 => bit_depth == 32 || bit_depth == 24,
        I16 | U16 => bit_depth == 16,
        I32 | U32 => bit_depth == 24 || bit_depth == 32,
        _ => false,
    }
}

/// Check whether one mixer card was never failed or failed long ago.
///
/// # Arguments
///
/// * `last_failure` - Last open failure, if any.
/// * `now` - Current time.
///
/// # Returns
///
/// * `bool` - Whether the open may proceed.
#[must_use]
pub fn mixer_retry_due(last_failure: Option<Instant>, now: Instant) -> bool {
    last_failure.is_none_or(|at| now.duration_since(at) >= MIXER_RETRY_INTERVAL)
}

/// Check whether one mixer card may be opened now.
///
/// First attempts always proceed; failures suppress repeats process-wide
/// until the retry interval lapses, so broken cards cost one libasound error
/// per interval instead of one per device open.
///
/// # Arguments
///
/// * `card` - ALSA card about to be opened.
///
/// # Returns
///
/// * `bool` - Whether the open may proceed.
#[must_use]
pub fn mixer_open_allowed(card: &str) -> bool {
    let last = {
        let failures = MIXER_FAILURES.lock();
        failures
            .iter()
            .find(|(name, _)| name.as_str() == card)
            .map(|(_, at)| *at)
    };
    mixer_retry_due(last, Instant::now())
}

/// Record one mixer open outcome; successes clear, failures start the TTL.
///
/// Replaces any existing record for the card, so repeated failures refresh a
/// single timestamp instead of stacking duplicates that would shadow the
/// retry check with stale entries.
///
/// # Arguments
///
/// * `card` - ALSA card that was opened.
/// * `ok` - Whether the open succeeded.
pub fn note_mixer_result(card: &str, ok: bool) {
    let mut failures = MIXER_FAILURES.lock();
    failures.retain(|(name, _)| name.as_str() != card);
    if !ok {
        failures.push((card.to_string(), Instant::now()));
    }
}

/// Attempt to initialise the ALSA hardware volume controller.
pub fn open_alsa_volume(device_id: &str) -> Option<AlsaVolumeControl> {
    let card = alsa_card_name(device_id);
    if !mixer_open_allowed(&card) {
        return None;
    }
    match AlsaVolumeControl::new(&card) {
        Ok(ctl) => {
            note_mixer_result(&card, true);
            info!(card = %card, "ALSA hardware volume control initialised");
            Some(ctl)
        }
        Err(e) => {
            note_mixer_result(&card, false);
            warn!(error = %e, "Failed to initialise ALSA volume control");
            None
        }
    }
}

/// Run one mixer helper and return its stdout on success.
///
/// # Arguments
///
/// * `program` - Helper binary such as `wpctl` or `pactl`.
/// * `args` - Arguments passed to the helper.
///
/// # Returns
///
/// * `Option<String>` - Stdout when the helper exits successfully.
fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let Ok(output) = Command::new(program).args(args).output() else {
        return None;
    };
    if !output.status.success() {
        return None;
    }
    let Ok(text) = String::from_utf8(output.stdout) else {
        return None;
    };
    Some(text)
}

/// Parse one `wpctl get-volume` line into a slider level plus mute state.
///
/// Accepts `Volume: 0.50` and `Volume: 0.00 [MUTED]`; levels clamp to
/// 0.0–1.0 so `PipeWire` boosts above 100% report unity rather than guessing.
///
/// # Arguments
///
/// * `output` - Stdout from `wpctl get-volume`.
///
/// # Returns
///
/// * `Option<(f64, bool)>` - Level plus mute state when parseable.
fn parse_wpctl_volume(output: &str) -> Option<(f64, bool)> {
    let (_, tail) = output.split_once("Volume:")?;
    let muted = tail.contains("MUTED");
    for token in tail.split_whitespace() {
        if let Ok(level) = token.parse::<f64>() {
            return Some((level.clamp(0.0, 1.0), muted));
        }
    }
    None
}

/// Parse one `pactl get-sink-volume` output into a slider level.
///
/// Averages every `NN%` channel percentage so multi-channel sinks report one
/// level; the mean clamps to 0.0–1.0.
///
/// # Arguments
///
/// * `output` - Stdout from `pactl get-sink-volume`.
///
/// # Returns
///
/// * `Option<f64>` - Slider level when at least one percentage parses.
fn parse_pactl_volume(output: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut count = 0_u32;
    let mut segments = output.split('%');
    let mut pending = segments.next()?;
    for rest in segments {
        let trailing = pending
            .rsplit(|candidate: char| !(candidate.is_ascii_digit() || candidate == '.'))
            .find(|token| !token.is_empty());
        if let Some(token) = trailing
            && let Ok(percent) = token.parse::<f64>()
        {
            total += percent;
            count = count.saturating_add(1);
        }
        pending = rest;
    }
    if count == 0 {
        return None;
    }
    Some((total / f64::from(count) / 100.0).clamp(0.0, 1.0))
}

/// Parse one `pactl get-sink-mute` output into a mute state.
///
/// # Arguments
///
/// * `output` - Stdout from `pactl get-sink-mute`.
///
/// # Returns
///
/// * `bool` - Whether the sink reports muted.
fn parse_pactl_mute(output: &str) -> bool {
    output.to_lowercase().contains("yes")
}

/// Sample the PipeWire/PulseAudio native system mixer; never fails.
///
/// Tries single-call `wpctl` first (volume plus mute), then two-call `pactl`
/// (volume plus mute). Unreadable setups return [`MixerSample::unknown`], so
/// the chain discloses `unknown` instead of guessing. Runs on the mixer tick
/// thread only, never on the GTK main thread.
///
/// # Returns
///
/// * `MixerSample` - System-side levels with [`PipeWirePulse`] source, or unknown when neither
///   helper reports a level.
#[must_use]
pub fn sample_pipewire_system() -> MixerSample {
    if let Some(output) = command_output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])
        && let Some((level, muted)) = parse_wpctl_volume(&output)
    {
        return MixerSample {
            device_volume: None,
            device_muted: false,
            system_volume: Some(level),
            system_muted: muted,
            system_source: PipeWirePulse,
        };
    }
    let Some(volume_output) = command_output("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])
    else {
        return MixerSample::unknown();
    };
    let Some(level) = parse_pactl_volume(&volume_output) else {
        return MixerSample::unknown();
    };
    let muted = command_output("pactl", &["get-sink-mute", "@DEFAULT_SINK@"])
        .is_some_and(|output| parse_pactl_mute(&output));
    MixerSample {
        device_volume: None,
        device_muted: false,
        system_volume: Some(level),
        system_muted: muted,
        system_source: PipeWirePulse,
    }
}

/// Check whether one native sample carries a readable system level.
///
/// # Arguments
///
/// * `sample` - Candidate native mixer sample.
///
/// # Returns
///
/// * `bool` - Whether the sample reports a native level.
#[must_use]
pub const fn native_level_readable(sample: &MixerSample) -> bool {
    matches!(sample.system_source, PipeWirePulse) && sample.system_volume.is_some()
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail, ensure};

    use crate::playback::{
        alsa_mixer::{
            MixerSample,
            SystemMixerSource::{PipeWirePulse, Unknown},
        },
        native_support::{
            MIXER_FAILURES, mixer_open_allowed, native_level_readable, note_mixer_result,
            parse_pactl_mute, parse_pactl_volume, parse_wpctl_volume, sample_pipewire_system,
        },
    };

    #[test]
    fn mixer_gate_suppresses_recent_failures() -> Result<()> {
        let card = "gate-probe-native";
        ensure!(mixer_open_allowed(card), "never attempted opens");
        note_mixer_result(card, false);
        ensure!(!mixer_open_allowed(card), "recent failure waits");
        note_mixer_result(card, true);
        ensure!(mixer_open_allowed(card), "success clears");
        Ok(())
    }

    #[test]
    fn mixer_failures_never_duplicate_cards() -> Result<()> {
        let card = "gate-probe-dedupe";
        note_mixer_result(card, false);
        note_mixer_result(card, false);
        let count = {
            let failures = MIXER_FAILURES.lock();
            failures.iter().filter(|(name, _)| name == card).count()
        };
        ensure!(count == 1, "one record per card, got {count}");
        note_mixer_result(card, true);
        Ok(())
    }

    #[test]
    fn wpctl_output_parses_level_and_mute() -> Result<()> {
        let Some((level, muted)) = parse_wpctl_volume("Volume: 0.50\n") else {
            bail!("plain wpctl level must parse")
        };
        ensure!(
            (level - 0.5).abs() < 1e-9,
            "wpctl level parses, got {level}"
        );
        ensure!(!muted, "plain wpctl stays unmuted");
        let Some((silent, muted)) = parse_wpctl_volume("Volume: 0.00 [MUTED]\n") else {
            bail!("muted wpctl level must parse")
        };
        ensure!(silent == 0.0, "muted wpctl keeps silence, got {silent}");
        ensure!(muted, "wpctl mute flag parses");
        ensure!(
            parse_wpctl_volume("Volume: muted\n").is_none(),
            "words never parse"
        );
        ensure!(
            parse_wpctl_volume("no header\n").is_none(),
            "header required"
        );
        Ok(())
    }

    #[test]
    fn pactl_outputs_parse_levels_and_mute() -> Result<()> {
        let stereo =
            "Volume: front-left: 32768 /  50% / -18.06 dB, front-right: 32768 /  50% / -18.06 dB";
        let Some(level) = parse_pactl_volume(stereo) else {
            bail!("stereo pactl must parse")
        };
        ensure!((level - 0.5).abs() < 1e-9, "stereo averages, got {level}");
        ensure!(
            parse_pactl_volume("Volume: no percent here").is_none(),
            "missing percent fails"
        );
        ensure!(parse_pactl_mute("Mute: yes\n"), "pactl yes mutes");
        ensure!(!parse_pactl_mute("Mute: no\n"), "pactl no stays unmuted");
        Ok(())
    }

    #[test]
    fn native_probe_never_fails_and_marks_readability() -> Result<()> {
        let sample = sample_pipewire_system();
        let readable = native_level_readable(&sample);
        if readable {
            ensure!(sample.system_volume.is_some(), "readable carries a level");
        } else {
            ensure!(
                sample.system_source == Unknown || sample.system_volume.is_none(),
                "unreadable stays unknown"
            );
        }
        let native = MixerSample {
            device_volume: None,
            device_muted: false,
            system_volume: Some(0.5),
            system_muted: false,
            system_source: PipeWirePulse,
        };
        ensure!(native_level_readable(&native), "native level reads");
        ensure!(
            !native_level_readable(&MixerSample::unknown()),
            "unknown never reads"
        );
        Ok(())
    }
}

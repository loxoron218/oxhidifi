//! Volume mapping: 0.0–1.0 slider → dB attenuation → linear gain.
//!
//! FR-020 specifies the slider range 0.0–1.0 mapped to dB attenuation. This
//! module implements the canonical curve used by both the software volume path
//! (`src/playback/stream.rs`) and the ALSA hardware volume path
//! (`src/playback/alsa_mixer.rs`). The player panel slider remains linear
//! 0.0–1.0; the dB conversion is applied only on the audio path.

use num_traits::cast::cast;

/// Minimum attenuation in decibels when the slider is at 0.0.
///
/// -60 dB corresponds to a linear gain of 0.001, effectively silence while
/// remaining representable for the ALSA mixer. Exact 0.0 returns gain 0.0
/// (digital silence) rather than -60 dB to ensure mute-at-zero.
const MIN_DB: f64 = -60.0;

/// Convert a linear slider value (0.0–1.0) to a linear gain (0.0–1.0) via a dB curve.
///
/// * `volume` ≤ 0.0 → 0.0 (silence)
/// * `volume` ≥ 1.0 → 1.0 (0 dB, unity)
/// * otherwise `dB = (volume - 1.0) * 60.0` (i.e., -60 dB at 0.0, 0 dB at 1.0), `gain =
///   10^(dB/20)`.
#[must_use]
pub fn volume_to_gain(volume: f64) -> f64 {
    if volume <= 0.0 {
        0.0
    } else if volume >= 1.0 {
        1.0
    } else {
        let db = (volume - 1.0) * -MIN_DB;
        let gain = 10.0_f64.powf(db / 20.0);
        gain.clamp(0.0, 1.0)
    }
}

/// Convert a linear slider value (0.0–1.0) to a linear gain as `f32` via the dB curve.
///
/// Convenience for the CPAL audio callback which operates on `f32` samples.
/// Takes an `f32` slider value to avoid `f64`→`f32` truncation on the hot path.
#[must_use]
pub fn volume_to_gain_f32(volume: f32) -> f32 {
    if volume <= 0.0 {
        0.0
    } else if volume >= 1.0 {
        1.0
    } else {
        let volume_f64 = f64::from(volume);
        let db = (volume_f64 - 1.0) * -MIN_DB;
        let gain = 10.0_f64.powf(db / 20.0);
        let gain_clamped = gain.clamp(0.0, 1.0);
        cast::<f64, f32>(gain_clamped).unwrap_or(0.0)
    }
}

/// Convert a linear slider value to a dB attenuation value for display.
///
/// Returns `f64::NEG_INFINITY` for `volume` ≤ 0.0 (true silence), otherwise
/// `(volume - 1.0) * 60.0`.
#[must_use]
pub fn volume_to_db(volume: f64) -> f64 {
    if volume <= 0.0 {
        f64::NEG_INFINITY
    } else if volume >= 1.0 {
        0.0
    } else {
        (volume - 1.0) * -MIN_DB
    }
}

/// Format a volume slider value as a human-readable dB string for tooltips.
///
/// * 0.0 → "-∞ dB"
/// * 1.0 → "0 dB"
/// * otherwise e.g., "-12.3 dB"
#[must_use]
pub fn format_volume_db(volume: f64) -> String {
    let db = volume_to_db(volume);
    if db.is_infinite() {
        "-∞ dB".to_string()
    } else if db == 0.0 {
        "0 dB".to_string()
    } else {
        format!("{db:.1} dB")
    }
}

#[cfg(test)]
mod tests {
    use crate::playback::volume::{format_volume_db, volume_to_db, volume_to_gain};

    #[test]
    fn volume_to_gain_endpoints() {
        assert!((volume_to_gain(0.0) - 0.0).abs() < f64::EPSILON);
        assert!((volume_to_gain(1.0) - 1.0).abs() < f64::EPSILON);
        assert!((volume_to_gain(-0.5) - 0.0).abs() < f64::EPSILON);
        assert!((volume_to_gain(2.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn volume_to_gain_midpoint_is_minus_30db() {
        let gain = volume_to_gain(0.5);
        let expected = 10.0_f64.powf(-30.0 / 20.0);
        assert!(
            (gain - expected).abs() < 1e-9,
            "0.5 → -30 dB, gain {gain} != {expected}"
        );
    }

    #[test]
    fn volume_to_gain_quarter_is_minus_45db() {
        let gain = volume_to_gain(0.25);
        let expected = 10.0_f64.powf(-45.0 / 20.0);
        assert!((gain - expected).abs() < 1e-9);
    }

    #[test]
    fn volume_to_db_endpoints() {
        assert!(volume_to_db(0.0).is_infinite() && volume_to_db(0.0).is_sign_negative());
        assert!((volume_to_db(1.0) - 0.0).abs() < f64::EPSILON);
        assert!((volume_to_db(0.5) - -30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn format_volume_db_cases() {
        assert_eq!(format_volume_db(0.0), "-∞ dB");
        assert_eq!(format_volume_db(1.0), "0 dB");
        assert_eq!(format_volume_db(0.5), "-30.0 dB");
    }
}

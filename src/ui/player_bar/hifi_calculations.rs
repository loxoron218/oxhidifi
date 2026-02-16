//! Hi-Fi quality calculations and state management.

use libadwaita::{gtk::Label, prelude::WidgetExt};

use crate::audio::{
    engine::{AudioEngine, TrackInfo},
    output::OutputConfig,
};

/// Maximum acceptable buffer duration indicating a lossy/high-latency device.
/// Values above this threshold suggest the audio path is not optimized
/// for bit-perfect playback (typical for Bluetooth or wireless devices).
const LOSSY_BUFFER_THRESHOLD_MS: u32 = 200;

/// Minimum bit depth for lossless CD-quality audio.
/// Anything below this indicates a compressed or reduced-quality audio path.
const CD_QUALITY_BITS: u32 = 16;

/// Hi-Fi quality state information.
#[derive(Clone, Copy)]
pub struct HifiQualityState {
    /// Whether audio is bit-perfect.
    pub bit_perfect: bool,
    /// Whether format conversion is active (bit depth change without resampling).
    pub format_conversion: bool,
}

/// Context struct for Hi-Fi popover label widgets.
pub struct HifiPopoverWidgets<'a> {
    /// Source format label in popover.
    pub source_format: &'a Label,
    /// Source sample rate label in popover.
    pub source_rate: &'a Label,
    /// Source bit depth label in popover.
    pub source_bits: &'a Label,
    /// Processing status label in popover.
    pub processing: &'a Label,
    /// Output device label in popover.
    pub output_device: &'a Label,
    /// Output format label in popover.
    pub output_format: &'a Label,
}

/// Calculates bit-perfect status based on track and output configuration.
///
/// # Arguments
///
/// * `track_info` - Optional track information
/// * `output_config` - Optional output configuration
///
/// # Returns
///
/// True if audio is bit-perfect, false otherwise.
#[must_use]
pub fn calculate_bit_perfect(
    track_info: Option<&TrackInfo>,
    output_config: Option<&OutputConfig>,
) -> bool {
    match (track_info, output_config) {
        (Some(track), Some(config)) => {
            let exclusive_mode = config.exclusive_mode;
            let no_resampling = !config.is_resampling;
            let format_matches = config.sample_rate == track.format.sample_rate;
            let bit_depth_matches = config.bits_per_sample == track.format.bits_per_sample;

            exclusive_mode && no_resampling && format_matches && bit_depth_matches
        }
        _ => false,
    }
}

/// Checks if format conversion is active (bit depth change without resampling).
///
/// # Arguments
///
/// * `track_info` - Optional track information
/// * `output_config` - Optional output configuration
///
/// # Returns
///
/// True if format conversion is active, false otherwise.
#[must_use]
pub fn is_format_conversion_active(
    track_info: Option<&TrackInfo>,
    output_config: Option<&OutputConfig>,
) -> bool {
    match (track_info, output_config) {
        (Some(track), Some(config)) => {
            let no_resampling = !config.is_resampling;
            let format_matches = config.sample_rate == track.format.sample_rate;
            let bit_depth_matches = config.bits_per_sample == track.format.bits_per_sample;

            no_resampling && format_matches && !bit_depth_matches
        }
        _ => false,
    }
}

/// Calculates gapless playback status.
///
/// # Arguments
///
/// * `audio_engine` - Audio engine reference
///
/// # Returns
///
/// True if gapless playback is active, false otherwise.
#[must_use]
pub fn calculate_gapless(audio_engine: &AudioEngine) -> bool {
    audio_engine.is_prebuffer_active()
}

/// Calculates Hi-Res status based on track sample rate.
///
/// # Arguments
///
/// * `track_info` - Optional track information
///
/// # Returns
///
/// True if track is Hi-Res (sample rate >= 48 kHz), false otherwise.
#[must_use]
pub fn calculate_hires(track_info: Option<&TrackInfo>) -> bool {
    match track_info {
        Some(track) => track.format.sample_rate >= 48000,
        None => false,
    }
}

/// Detects if the output device is lossy (Bluetooth, high latency, etc.).
///
/// # Arguments
///
/// * `output_config` - Output configuration reference
///
/// # Returns
///
/// True if output device is lossy, false otherwise.
#[must_use]
pub fn is_lossy_device(output_config: &OutputConfig) -> bool {
    if let Some(ref device_name) = output_config.device_name {
        let device_lower = device_name.to_lowercase();
        return device_lower.contains("bluetooth")
            || device_lower.contains("airpods")
            || (device_lower.contains("bluetooth") && device_lower.contains("head"))
            || device_lower.contains("wireless")
            || device_lower.contains("sbc")
            || device_lower.contains("aac");
    }

    if output_config.buffer_duration_ms > LOSSY_BUFFER_THRESHOLD_MS {
        return true;
    }

    if output_config.bits_per_sample < CD_QUALITY_BITS {
        return true;
    }

    false
}

/// Updates the Hi-Fi popover labels with current audio information.
///
/// # Arguments
///
/// * `track_info` - Optional track information
/// * `output_config` - Optional output configuration
/// * `quality_state` - Hi-Fi quality state information
/// * `popover_widgets` - Context struct with popover label references
pub fn update_hifi_popover_labels(
    track_info: Option<&TrackInfo>,
    output_config: Option<&OutputConfig>,
    quality_state: HifiQualityState,
    popover_widgets: &HifiPopoverWidgets<'_>,
) {
    if let Some(track) = track_info {
        popover_widgets
            .source_format
            .set_label(&track.metadata.technical.format);

        let sample_rate_display = if track.format.sample_rate % 1000 == 0 {
            format!("{} kHz", track.format.sample_rate / 1000)
        } else {
            format!("{:.1} kHz", f64::from(track.format.sample_rate) / 1000.0)
        };
        popover_widgets.source_rate.set_label(&sample_rate_display);

        popover_widgets
            .source_bits
            .set_label(&format!("{}-bit", track.format.bits_per_sample));

        let is_exclusive = output_config.is_some_and(|c| c.exclusive_mode && !c.is_resampling);
        let mode_str = if is_exclusive {
            "(Exclusive)"
        } else {
            "(Shared)"
        };
        let processing_status = if quality_state.bit_perfect {
            format!("Direct {mode_str}")
        } else if quality_state.format_conversion {
            format!("Format conversion {mode_str}")
        } else {
            format!("Resampling {mode_str}")
        };
        popover_widgets.processing.set_label(&processing_status);
    } else {
        popover_widgets.source_format.set_label("-");
        popover_widgets.source_rate.set_label("-");
        popover_widgets.source_bits.set_label("-");
        popover_widgets.processing.set_label("-");
    }

    if let Some(config) = output_config {
        popover_widgets.output_device.set_label(
            &config
                .device_name
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
        );

        let format_str = format!(
            "{}-bit / {} kHz",
            config.bits_per_sample,
            config.sample_rate / 1000
        );
        popover_widgets.output_format.set_label(&format_str);
    } else {
        popover_widgets.output_device.set_label("-");
        popover_widgets.output_format.set_label("-");
    }
}

/// Updates a badge widget's CSS class based on active state.
///
/// # Arguments
///
/// * `badge` - Badge label widget
/// * `active` - Whether the badge should be active
pub fn update_badge_css_class(badge: &Label, active: bool) {
    if active {
        badge.remove_css_class("inactive");
    } else {
        badge.add_css_class("inactive");
    }
}

/// Calculates the Hi-Fi button CSS class based on audio state.
///
/// # Arguments
///
/// * `track_info` - Optional track information
/// * `is_bit_perfect` - Whether audio is bit-perfect
/// * `is_gapless` - Whether gapless playback is active
/// * `is_lossy` - Whether output device is lossy
///
/// # Returns
///
/// The CSS class name for the button state.
#[must_use]
pub fn calculate_hifi_button_class(
    track_info: Option<&TrackInfo>,
    is_bit_perfect: bool,
    is_gapless: bool,
    is_lossy: bool,
) -> &'static str {
    if track_info.is_none() {
        "hifi-inactive"
    } else if is_lossy {
        "hifi-lossy"
    } else if is_bit_perfect && is_gapless {
        "hifi-perfect"
    } else if is_bit_perfect {
        "hifi-good"
    } else {
        "hifi-compromised"
    }
}

//! Unit tests for Hi-Fi quality calculations.

use crate::{
    audio::{
        decoder::AudioFormat,
        engine::TrackInfo,
        metadata::{StandardMetadata, TechnicalMetadata, TrackMetadata},
        output::OutputConfig,
    },
    ui::player_bar::hifi_calculations::{
        calculate_bit_perfect, calculate_hifi_button_class, calculate_hires,
        is_format_conversion_active, is_lossy_device,
    },
};

fn create_test_track_info(sample_rate: u32, bits_per_sample: u32) -> TrackInfo {
    TrackInfo {
        path: "/test/music.flac".to_string(),
        metadata: TrackMetadata {
            standard: StandardMetadata {
                title: Some("Test Track".to_string()),
                artist: Some("Test Artist".to_string()),
                album: Some("Test Album".to_string()),
                album_artist: None,
                track_number: Some(1),
                total_tracks: None,
                disc_number: None,
                total_discs: None,
                year: None,
                genre: None,
                comment: None,
            },
            technical: TechnicalMetadata {
                format: "FLAC".to_string(),
                codec: "FLAC".to_string(),
                sample_rate,
                bits_per_sample,
                channels: 2,
                duration_ms: 300_000,
                file_size: 25_000_000,
                is_lossless: true,
                is_high_resolution: sample_rate >= 48000,
            },
            artwork: None,
        },
        format: AudioFormat {
            sample_rate,
            channels: 2,
            bits_per_sample,
            channel_mask: 0,
        },
        duration_ms: 300_000,
    }
}

fn create_test_output_config(
    sample_rate: u32,
    bits_per_sample: u32,
    exclusive_mode: bool,
    is_resampling: bool,
    device_name: Option<String>,
    buffer_duration_ms: u32,
) -> OutputConfig {
    OutputConfig {
        sample_rate,
        channels: 2,
        buffer_duration_ms,
        exclusive_mode,
        device_name,
        bits_per_sample,
        is_resampling,
    }
}

#[test]
fn test_calculate_bit_perfect_both_none() {
    assert!(!calculate_bit_perfect(None, None));
}

#[test]
fn test_calculate_bit_perfect_track_none() {
    let config = create_test_output_config(44100, 16, true, false, None, 50);
    assert!(!calculate_bit_perfect(None, Some(&config)));
}

#[test]
fn test_calculate_bit_perfect_config_none() {
    let track = create_test_track_info(44100, 16);
    assert!(!calculate_bit_perfect(Some(&track), None));
}

#[test]
fn test_calculate_bit_perfect_all_matching() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 16, true, false, None, 50);
    assert!(calculate_bit_perfect(Some(&track), Some(&config)));
}

#[test]
fn test_calculate_bit_perfect_not_exclusive() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 16, false, false, None, 50);
    assert!(!calculate_bit_perfect(Some(&track), Some(&config)));
}

#[test]
fn test_calculate_bit_perfect_resampling_active() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 16, true, true, None, 50);
    assert!(!calculate_bit_perfect(Some(&track), Some(&config)));
}

#[test]
fn test_calculate_bit_perfect_sample_rate_mismatch() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(48000, 16, true, false, None, 50);
    assert!(!calculate_bit_perfect(Some(&track), Some(&config)));
}

#[test]
fn test_calculate_bit_perfect_bit_depth_mismatch() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 24, true, false, None, 50);
    assert!(!calculate_bit_perfect(Some(&track), Some(&config)));
}

#[test]
fn test_calculate_bit_perfect_all_mismatch() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(96000, 24, false, true, None, 50);
    assert!(!calculate_bit_perfect(Some(&track), Some(&config)));
}

#[test]
fn test_is_format_conversion_active_both_none() {
    assert!(!is_format_conversion_active(None, None));
}

#[test]
fn test_is_format_conversion_active_track_none() {
    let config = create_test_output_config(44100, 24, true, false, None, 50);
    assert!(!is_format_conversion_active(None, Some(&config)));
}

#[test]
fn test_is_format_conversion_active_config_none() {
    let track = create_test_track_info(44100, 16);
    assert!(!is_format_conversion_active(Some(&track), None));
}

#[test]
fn test_is_format_conversion_active_true() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 24, true, false, None, 50);
    assert!(is_format_conversion_active(Some(&track), Some(&config)));
}

#[test]
fn test_is_format_conversion_active_same_bits() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 16, true, false, None, 50);
    assert!(!is_format_conversion_active(Some(&track), Some(&config)));
}

#[test]
fn test_is_format_conversion_active_resampling() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(44100, 24, true, true, None, 50);
    assert!(!is_format_conversion_active(Some(&track), Some(&config)));
}

#[test]
fn test_is_format_conversion_active_sample_rate_mismatch() {
    let track = create_test_track_info(44100, 16);
    let config = create_test_output_config(48000, 24, true, false, None, 50);
    assert!(!is_format_conversion_active(Some(&track), Some(&config)));
}

#[test]
fn test_calculate_hires_none() {
    assert!(!calculate_hires(None));
}

#[test]
fn test_calculate_hires_at_threshold() {
    let track = create_test_track_info(48000, 16);
    assert!(calculate_hires(Some(&track)));
}

#[test]
fn test_calculate_hires_above_threshold() {
    let track = create_test_track_info(96000, 24);
    assert!(calculate_hires(Some(&track)));
}

#[test]
fn test_calculate_hires_below_threshold() {
    let track = create_test_track_info(44100, 16);
    assert!(!calculate_hires(Some(&track)));
}

#[test]
fn test_calculate_hires_very_high() {
    let track = create_test_track_info(192_000, 32);
    assert!(calculate_hires(Some(&track)));
}

#[test]
fn test_is_lossy_device_bluetooth() {
    let config = create_test_output_config(
        44100,
        16,
        false,
        false,
        Some("Bluetooth Headphones".to_string()),
        50,
    );
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_airpods() {
    let config =
        create_test_output_config(44100, 16, false, false, Some("AirPods Pro".to_string()), 50);
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_bluetooth_headset() {
    let config = create_test_output_config(
        44100,
        16,
        false,
        false,
        Some("Bluetooth Headset".to_string()),
        50,
    );
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_wireless() {
    let config = create_test_output_config(
        44100,
        16,
        false,
        false,
        Some("Wireless Speaker".to_string()),
        50,
    );
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_sbc() {
    let config = create_test_output_config(
        44100,
        16,
        false,
        false,
        Some("SBC Codec Device".to_string()),
        50,
    );
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_aac() {
    let config =
        create_test_output_config(44100, 16, false, false, Some("AAC Support".to_string()), 50);
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_case_insensitive() {
    let config = create_test_output_config(
        44100,
        16,
        false,
        false,
        Some("BLUETOOTH SPEAKER".to_string()),
        50,
    );
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_wired() {
    let config = create_test_output_config(
        44100,
        24,
        false,
        false,
        Some("USB Audio Interface".to_string()),
        50,
    );
    assert!(!is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_high_latency() {
    let config = create_test_output_config(44100, 24, false, false, None, 250);
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_latency_at_threshold() {
    let config = create_test_output_config(44100, 24, false, false, None, 200);
    assert!(!is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_low_latency() {
    let config = create_test_output_config(44100, 24, false, false, None, 50);
    assert!(!is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_low_bit_depth() {
    let config = create_test_output_config(44100, 8, false, false, None, 50);
    assert!(is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_16bit() {
    let config = create_test_output_config(44100, 16, false, false, None, 50);
    assert!(!is_lossy_device(&config));
}

#[test]
fn test_is_lossy_device_no_name_low_bits() {
    let config = create_test_output_config(44100, 8, false, false, None, 50);
    assert!(is_lossy_device(&config));
}

#[test]
fn test_calculate_hifi_button_class_no_track() {
    assert_eq!(
        calculate_hifi_button_class(None, true, true, false),
        "hifi-inactive"
    );
}

#[test]
fn test_calculate_hifi_button_class_lossy() {
    let track = create_test_track_info(44100, 16);
    assert_eq!(
        calculate_hifi_button_class(Some(&track), true, true, true),
        "hifi-lossy"
    );
}

#[test]
fn test_calculate_hifi_button_class_perfect_and_gapless() {
    let track = create_test_track_info(44100, 16);
    assert_eq!(
        calculate_hifi_button_class(Some(&track), true, true, false),
        "hifi-perfect"
    );
}

#[test]
fn test_calculate_hifi_button_class_perfect_no_gapless() {
    let track = create_test_track_info(44100, 16);
    assert_eq!(
        calculate_hifi_button_class(Some(&track), true, false, false),
        "hifi-good"
    );
}

#[test]
fn test_calculate_hifi_button_class_not_perfect() {
    let track = create_test_track_info(44100, 16);
    assert_eq!(
        calculate_hifi_button_class(Some(&track), false, true, false),
        "hifi-compromised"
    );
}

#[test]
fn test_calculate_hifi_button_class_compromised_no_gapless() {
    let track = create_test_track_info(44100, 16);
    assert_eq!(
        calculate_hifi_button_class(Some(&track), false, false, false),
        "hifi-compromised"
    );
}

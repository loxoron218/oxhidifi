//! Utility functions for UI components.
//!
//! This module provides shared utility functions for UI components,
//! including proper format display logic that adheres to fidelity principles.

use crate::library::models::Album;

/// Formats a sample rate in Hz to a clean kHz string representation.
///
/// This function uses consistent integer arithmetic to handle all sample rates uniformly:
/// - 48000 Hz → "48"
/// - 44100 Hz → "44.1"
/// - 22050 Hz → "22.05"
/// - 44123 Hz → "44.123"
///
/// The implementation avoids hardcoded matches and nested conditionals by using
/// padded formatting for the remainder and trimming trailing zeros while preserving
/// whole-number outputs as integers.
///
/// # Arguments
///
/// * `sample_rate_hz` - The sample rate in Hertz
///
/// # Returns
///
/// A formatted string representing the sample rate in kHz (without "kHz" unit)
pub fn format_sample_rate(sample_rate_hz: i64) -> String {
    let whole_khz = sample_rate_hz / 1000;
    let remainder = sample_rate_hz % 1000;

    if remainder == 0 {
        // Whole number kHz (e.g., 48000 -> "48")
        whole_khz.to_string()
    } else {
        // Format remainder with leading zeros to ensure correct decimal placement
        // e.g., 100 -> "100", 50 -> "050", 23 -> "023"
        let remainder_str = format!("{:03}", remainder);

        // Combine whole and fractional parts
        let mut result = format!("{}.{}", whole_khz, remainder_str);

        // Trim trailing zeros from the fractional part
        // Find the position of the decimal point
        if let Some(decimal_pos) = result.find('.') {
            // Remove trailing zeros
            let trimmed = result.trim_end_matches('0');
            if trimmed.len() > decimal_pos + 1 {
                // Keep at least one digit after decimal point
                result.truncate(trimmed.len());
            } else {
                // All fractional digits were zeros, return just the whole number
                result.truncate(decimal_pos);
            }
        }

        result
    }
}

/// Creates a user-friendly format display string from album metadata.
///
/// This function creates a display string that accurately represents the audio format
/// without misleading the user. It follows these rules:
/// - If complete format information is available (format, bits_per_sample, sample_rate),
///   displays as "FORMAT bits/sample_rate_khz" (e.g., "FLAC 24/96", "FLAC 24/44.1")
/// - If only format name is available, displays just the format name (e.g., "FLAC")
/// - If no format metadata is available, attempts to infer from file extension
/// - If format cannot be determined, returns None to indicate format should not be displayed
///
/// # Arguments
///
/// * `album` - The album containing format metadata
///
/// # Returns
///
/// An `Option<String>` containing the formatted display string, or `None` if format
/// cannot be determined and should not be displayed.
pub fn create_format_display(album: &Album) -> Option<String> {
    // Use actual format metadata if available
    if let Some(ref format_name) = album.format {
        if let (Some(bits), Some(sample_rate)) = (album.bits_per_sample, album.sample_rate) {
            // Format as "FORMAT bits/sample_rate" (e.g., "FLAC 24/96", "FLAC 24/44.1")
            // Convert sample_rate from Hz to kHz for display with proper decimal handling
            let sample_rate_khz = format_sample_rate(sample_rate);
            return Some(format!("{} {}/{}", format_name, bits, sample_rate_khz));
        } else {
            // Only format name available
            return Some(format_name.clone());
        }
    }

    // Fallback to file extension inference if no format metadata
    let path_lower = album.path.to_lowercase();
    if path_lower.ends_with(".flac") {
        Some("FLAC".to_string())
    } else if path_lower.ends_with(".wav") {
        Some("WAV".to_string())
    } else if path_lower.ends_with(".aiff") || path_lower.ends_with(".aif") {
        Some("AIFF".to_string())
    } else if path_lower.ends_with(".dsf") || path_lower.ends_with(".dff") {
        Some("DSD".to_string())
    } else if path_lower.ends_with(".mqa") {
        Some("MQA".to_string())
    } else if path_lower.ends_with(".mp3") {
        Some("MP3".to_string())
    } else if path_lower.ends_with(".aac") {
        Some("AAC".to_string())
    } else if path_lower.ends_with(".ogg") || path_lower.ends_with(".oga") {
        Some("Ogg".to_string())
    } else if path_lower.ends_with(".opus") {
        Some("Opus".to_string())
    } else if path_lower.ends_with(".wv") {
        Some("WavPack".to_string())
    } else if path_lower.ends_with(".ape") {
        Some("Monkey's Audio".to_string())
    } else {
        // Format cannot be determined - return None to indicate it should not be displayed
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::utils::{Album, create_format_display, format_sample_rate};

    #[test]
    fn test_format_sample_rate_common_rates() {
        // Test common decimal sample rates
        assert_eq!(format_sample_rate(44100), "44.1");
        assert_eq!(format_sample_rate(88200), "88.2");
        assert_eq!(format_sample_rate(176400), "176.4");

        // Test common whole number sample rates
        assert_eq!(format_sample_rate(8000), "8");
        assert_eq!(format_sample_rate(16000), "16");
        assert_eq!(format_sample_rate(24000), "24");
        assert_eq!(format_sample_rate(32000), "32");
        assert_eq!(format_sample_rate(48000), "48");
        assert_eq!(format_sample_rate(96000), "96");
        assert_eq!(format_sample_rate(192000), "192");
        assert_eq!(format_sample_rate(384000), "384");

        // Test lower sample rates with decimals
        assert_eq!(format_sample_rate(11025), "11.025");
        assert_eq!(format_sample_rate(22050), "22.05");
    }

    #[test]
    fn test_format_sample_rate_uncommon_rates() {
        // Test uncommon sample rates that require floating point formatting
        assert_eq!(format_sample_rate(44000), "44");
        assert_eq!(format_sample_rate(44123), "44.123");
        assert_eq!(format_sample_rate(48100), "48.1");
        assert_eq!(format_sample_rate(95900), "95.9");
        assert_eq!(format_sample_rate(44120), "44.12");
        assert_eq!(format_sample_rate(123456), "123.456");
    }

    #[test]
    fn test_complete_format_metadata() {
        let album = Album {
            format: Some("FLAC".to_string()),
            bits_per_sample: Some(24),
            sample_rate: Some(44100),
            path: "/path/to/album".to_string(),
            ..Album::default()
        };
        assert_eq!(
            create_format_display(&album),
            Some("FLAC 24/44.1".to_string())
        );
    }

    #[test]
    fn test_complete_format_metadata_whole_number() {
        let album = Album {
            format: Some("FLAC".to_string()),
            bits_per_sample: Some(24),
            sample_rate: Some(96000),
            path: "/path/to/album".to_string(),
            ..Album::default()
        };
        assert_eq!(
            create_format_display(&album),
            Some("FLAC 24/96".to_string())
        );
    }

    #[test]
    fn test_format_only_metadata() {
        let album = Album {
            format: Some("MP3".to_string()),
            bits_per_sample: None,
            sample_rate: None,
            path: "/path/to/album".to_string(),
            ..Album::default()
        };
        assert_eq!(create_format_display(&album), Some("MP3".to_string()));
    }

    #[test]
    fn test_file_extension_fallback() {
        let album = Album {
            format: None,
            bits_per_sample: None,
            sample_rate: None,
            path: "/path/to/album.flac".to_string(),
            ..Album::default()
        };
        assert_eq!(create_format_display(&album), Some("FLAC".to_string()));
    }

    #[test]
    fn test_unknown_format_returns_none() {
        let album = Album {
            format: None,
            bits_per_sample: None,
            sample_rate: None,
            path: "/path/to/album.unknown".to_string(),
            ..Album::default()
        };
        assert_eq!(create_format_display(&album), None);
    }

    #[test]
    fn test_dsd_formats() {
        let album_dsf = Album {
            format: None,
            bits_per_sample: None,
            sample_rate: None,
            path: "/path/to/album.dsf".to_string(),
            ..Album::default()
        };
        assert_eq!(create_format_display(&album_dsf), Some("DSD".to_string()));

        let album_dff = Album {
            format: None,
            bits_per_sample: None,
            sample_rate: None,
            path: "/path/to/album.dff".to_string(),
            ..Album::default()
        };
        assert_eq!(create_format_display(&album_dff), Some("DSD".to_string()));
    }
}

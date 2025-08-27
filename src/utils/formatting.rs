/// Utilities for formatting various data types into human-readable strings.
///
/// This module provides functions to format durations, bit depth, frequencies,
/// and years for display within the application's user interface, ensuring consistency
/// and adherence to common presentation standards.

/// Formats a duration in seconds into a "H:MM:SS" string (e.g., 1:23:45).
///
/// This function is primarily used for displaying total album or track durations
/// where hours might be a relevant component.
///
/// # Arguments
/// * `total_seconds` - The total duration in seconds.
///
/// # Returns
/// A `String` representing the formatted duration.
pub(crate) fn format_duration_hms(total_seconds: u32) -> String {
    let h = total_seconds / 3600;
    let m = (total_seconds % 3600) / 60;
    let s = total_seconds % 60;
    format!("{:01}:{:02}:{:02}", h, m, s)
}

/// Formats a duration in seconds into an "MM:SS" string (e.g., 03:45).
///
/// This function is typically used for individual track durations where
/// hours are not expected or desired in the display.
///
/// # Arguments
/// * `secs` - The duration in seconds.
///
/// # Returns
/// A `String` representing the formatted duration.
pub(crate) fn format_duration_mmss(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Formats a frequency in Hertz (Hz) to a kilohertz (kHz) string.
///
/// This private helper function handles the common logic for converting and
/// formatting frequency values. It attempts to display an integer if the
/// kHz value is very close to a whole number, otherwise it shows one decimal place.
///
/// # Arguments
/// * `freq` - The frequency in Hertz (u32).
///
/// # Returns
/// A `String` representing the frequency in kHz.
fn format_khz_display(freq: u32) -> String {
    let khz = (freq as f32) / 1000.0;
    // Check if the kHz value is very close to a whole number.
    // Using a small epsilon to account for floating point inaccuracies.
    if khz.fract().abs() < 0.01 {
        format!("{:.0}", khz)
    } else {
        format!("{:.1}", khz)
    }
}

/// Formats frequency as kHz (e.g., "44.1 kHz").
///
/// This function is a public wrapper around `format_khz_display` and adds the " kHz" suffix.
/// It's used when only frequency information needs to be displayed.
///
/// # Arguments
/// * `freq` - The frequency in Hertz (u32).
///
/// # Returns
/// A `String` representing the frequency in kHz with units.
pub(crate) fn format_freq_khz(freq: u32) -> String {
    format!("{} kHz", format_khz_display(freq))
}

/// Formats bit depth and frequency into a combined string (e.g., "24-Bit/96 kHz").
///
/// This function handles various combinations of optional bit depth and frequency
/// values, providing a concise string representation. It leverages `format_khz_display`
/// for consistent frequency formatting.
///
/// # Arguments
/// * `bit` - An `Option<u32>` representing the bit depth.
/// * `freq` - An `Option<u32>` representing the frequency in Hertz.
///
/// # Returns
/// A `String` representing the formatted bit depth and frequency. Returns an empty
/// string if both are `None`.
pub(crate) fn format_bit_freq(bit: Option<u32>, freq: Option<u32>) -> String {
    let bit_str = bit.map(|b| format!("{}-Bit", b));
    let freq_str = freq.map(|f| format_khz_display(f)); // Use the helper function here

    match (bit_str, freq_str) {
        (Some(b), Some(f)) => format!("{}/{} kHz", b, f), // Add kHz suffix here
        (Some(b), None) => b,
        (None, Some(f)) => format!("{} kHz", f), // Add kHz suffix here
        (None, None) => String::new(),
    }
}

/// Formats year information for display, handling both release year and original release date.
///
/// This function provides a consistent way to format year information based on user preferences
/// for showing original release dates vs. release years. It handles various combinations of
/// available data and avoids duplicate year display.
///
/// # Arguments
/// * `release_year` - An `Option<i32>` representing the release year.
/// * `original_release_date` - An `Option<&str>` representing the original release date string.
/// * `use_original_year` - A `bool` indicating whether to prioritize original release date.
///
/// # Returns
/// A `String` representing the formatted year information.
pub(crate) fn format_year_info(
    release_year: Option<i32>,
    original_release_date: Option<&str>,
    use_original_year: bool,
) -> String {
    // Extract year from original release date if available
    let original_year = original_release_date
        .and_then(|date| date.split('-').next())
        .and_then(|year_str| year_str.parse::<i32>().ok())
        .map(|y| y.to_string());

    // Format based on user preference and available data
    match (use_original_year, original_year, release_year) {
        // Use original year when requested and available
        (true, Some(o_year), _) => o_year,
        // Fallback to release year when original year is not available or not requested
        (_, _, Some(r_year)) => r_year.to_string(),
        // Use original year as fallback when release year is not available
        (false, Some(o_year), None) => o_year,
        // No year information available
        _ => String::new(),
    }
}

/// Formats album year display with both original and release years when they differ.
///
/// This function creates a display string that shows both years when they're different,
/// helping users understand reissues, remasters, etc.
///
/// # Arguments
/// * `release_year` - An `Option<i32>` representing the release year.
/// * `original_release_date` - An `Option<&str>` representing the original release date string.
///
/// # Returns
/// A `String` representing the formatted year display.
pub(crate) fn format_album_year_display(
    release_year: Option<i32>,
    original_release_date: Option<&str>,
) -> String {
    let original_year = original_release_date
        .and_then(|date| date.split('-').next())
        .and_then(|year_str| year_str.parse::<i32>().ok());

    match (original_year, release_year) {
        (Some(o_year), Some(r_year)) => {
            if o_year == r_year {
                o_year.to_string()
            } else {
                format!("{} / {}", o_year, r_year)
            }
        }
        (Some(o_year), None) => o_year.to_string(),
        (None, Some(r_year)) => r_year.to_string(),
        _ => String::new(),
    }
}

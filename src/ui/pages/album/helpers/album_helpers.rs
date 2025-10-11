use std::collections::{HashMap, HashSet};

use crate::data::models::Song;

/// Determines the most common audio properties (bit depth, sample rate, and format) from a collection of songs.
///
/// This function analyzes a slice of [`Song`] objects to find the most frequently occurring
/// technical audio specifications. It's primarily used in the album detail view to display
/// representative technical information for the entire album.
///
/// # Parameters
///
/// * `songs` - A slice of [`Song`] objects to analyze
///
/// # Returns
///
/// A tuple containing:
/// * `Option<u32>` - The most common bit depth (e.g., 16, 24) or `None` if not available
/// * `Option<u32>` - The most common sample rate in Hz (e.g., 44100, 96000) or `None` if not available
/// * `Option<String>` - The most common format (e.g., "FLAC", "MP3") or `None` if not available
///
/// # Examples
///
/// ```
/// # use crate::data::models::Song;
/// # let songs = vec![]; // Vector of Song objects
/// let (bit_depth, sample_rate, format) = get_most_common_song_properties(&songs);
/// ```
///
/// # See Also
///
/// * [`Song`] - The data model containing song information
/// * [`crate::ui::pages::album::components::technical_info::build_technical_info`] - Where this function is used
pub fn get_most_common_song_properties(
    songs: &[Song],
) -> (Option<u32>, Option<u32>, Option<String>) {
    let mut bit_depth_counts: HashMap<u32, usize> = HashMap::new();
    let mut sample_rate_counts: HashMap<u32, usize> = HashMap::new();
    let mut format_counts: HashMap<String, usize> = HashMap::new();

    // Count occurrences of each property across all songs
    for song in songs {
        if let Some(bd) = song.bit_depth {
            *bit_depth_counts.entry(bd).or_insert(0) += 1;
        }
        if let Some(sr) = song.sample_rate {
            *sample_rate_counts.entry(sr).or_insert(0) += 1;
        }
        if let Some(fmt) = &song.format {
            *format_counts.entry(fmt.clone()).or_insert(0) += 1;
        }
    }

    // Find the most common value for each property
    let most_common_bit_depth = bit_depth_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(bd, _)| bd);
    let most_common_sample_rate = sample_rate_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(sr, _)| sr);
    let most_common_format = format_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(fmt, _)| fmt);
    (
        most_common_bit_depth,
        most_common_sample_rate,
        most_common_format,
    )
}

/// Determines if an album has mixed audio properties (different formats, bit depths, or sample rates).
///
/// This function analyzes a slice of [`Song`] objects to check if there are multiple
/// different technical audio specifications across the songs in an album.
///
/// # Parameters
///
/// * `songs` - A slice of [`Song`] objects to analyze
///
/// # Returns
///
/// A tuple containing:
/// * `bool` - True if there are multiple different bit depths, false otherwise
/// * `bool` - True if there are multiple different sample rates, false otherwise
/// * `bool` - True if there are multiple different formats, false otherwise
pub fn has_mixed_audio_properties(songs: &[Song]) -> (bool, bool, bool) {
    let mut bit_depths: HashSet<Option<u32>> = HashSet::new();
    let mut sample_rates: HashSet<Option<u32>> = HashSet::new();
    let mut formats: HashSet<Option<String>> = HashSet::new();
    for song in songs {
        bit_depths.insert(song.bit_depth);
        sample_rates.insert(song.sample_rate);
        formats.insert(song.format.clone());
    }

    // Check for mixed values including the presence/absence of values
    // If we have both Some and None values, that's mixed
    // If we have multiple different Some values, that's also mixed
    let has_mixed_bit_depths = bit_depths.len() > 1;
    let has_mixed_sample_rates = sample_rates.len() > 1;
    let has_mixed_formats = formats.len() > 1;
    (
        has_mixed_bit_depths,
        has_mixed_sample_rates,
        has_mixed_formats,
    )
}

/// Determines if an audio format is considered "lossy".
///
/// Lossy formats are those that use compression algorithms that discard some audio
/// data to reduce file size. This function is used to determine quality indicators
/// for albums in the UI.
///
/// # Parameters
///
/// * `format` - An optional string representing the audio format (e.g., "mp3", "flac")
///
/// # Returns
///
/// * `true` if the format is considered lossy
/// * `false` if the format is not lossy or if format information is not available
///
/// # Supported Lossy Formats
///
/// Currently recognized lossy formats:
/// * "mp3" - MPEG Audio Layer III
/// * "aac" - Advanced Audio Coding
/// * "ogg" - Ogg Vorbis
/// * "wma" - Windows Media Audio
///
/// # Examples
///
/// ```
/// assert_eq!(is_lossy_format(&Some("mp3".to_string())), true);
/// assert_eq!(is_lossy_format(&Some("flac".to_string())), false);
/// assert_eq!(is_lossy_format(&None), false);
/// ```
///
/// # See Also
///
/// * [`crate::ui::pages::album::components::technical_info::build_technical_info`] - Where this function is used
pub fn is_lossy_format(format: &Option<String>) -> bool {
    matches!(
        format.as_deref(),
        Some("mp3") | Some("aac") | Some("ogg") | Some("wma")
    )
}

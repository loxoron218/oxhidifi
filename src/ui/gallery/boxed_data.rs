//! Plain data types for `gtk::ColumnView` items.
//!
//! Wrapped in `BoxedAnyObject` for use with `gio::ListStore`.

use num_traits::cast::FromPrimitive;

/// Data for an album displayed in `GtkColumnView`.
#[derive(Clone, Debug)]
pub struct AlbumData {
    /// Unique album identifier.
    pub id: i64,
    /// Album title.
    pub title: String,
    /// Artist display name.
    pub artist_name: String,
    /// Release year (0 = unknown).
    pub year: i32,
    /// Audio format display (e.g. "FLAC" or "FLAC, MP3").
    pub format: String,
    /// Bit depth display (e.g. "24" or "16, 24").
    pub bit_depth: String,
    /// Sample rate display (e.g. "96" or "44.1, 96").
    pub sample_rate: String,
    /// Total album duration in seconds.
    pub duration_secs: f64,
    /// Formatted duration display (e.g. "42:15" or "1:12:30").
    pub duration: String,
    /// Path to album artwork (empty = no artwork).
    pub artwork_path: String,
}

/// Data for an artist displayed in `GtkColumnView`.
#[derive(Clone, Debug)]
pub struct ArtistData {
    /// Unique artist identifier.
    pub id: i64,
    /// Artist display name.
    pub name: String,
    /// Number of albums by this artist.
    pub album_count: i32,
}

/// Format album duration seconds as `M:SS` or `H:MM:SS`.
///
/// Rounds to the nearest second and formats with leading zeros for seconds
/// and minutes. Returns empty string for 0.0 or negative durations.
#[must_use]
pub fn format_duration(secs: f64) -> String {
    if secs <= 0.0 {
        return String::new();
    }
    let total = FromPrimitive::from_f64(secs.round()).unwrap_or(0_i64);
    let hours = total / 3600;
    let mins = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins}:{secs:02}")
    }
}

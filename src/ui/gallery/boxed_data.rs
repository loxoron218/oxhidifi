//! Plain data types for `gtk::ColumnView` items.
//!
//! Wrapped in `BoxedAnyObject` for use with `gio::ListStore`.

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

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
    /// Audio codec name (e.g. "FLAC", "MP3").
    pub format: String,
    /// Bit depth (0 = N/A for lossy formats).
    pub bit_depth: i32,
    /// Sample rate in Hz (0 = unknown).
    pub sample_rate: i32,
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

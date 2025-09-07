use std::path::PathBuf;

use sqlx::{Error, Row, SqlitePool, query};

/// Represents display information for an album on an artist page.
///
/// This struct contains all the information needed to display an album
/// in the context of an artist page, including basic metadata, audio
/// format information, and dynamic range data.
///
/// # Examples
///
/// ```
/// # use std::path::PathBuf;
/// # use crate::ui::pages::artist::data::artist_data::AlbumDisplayInfoWithYear;
/// let album = AlbumDisplayInfoWithYear {
///     id: 1,
///     title: "Album Title".to_string(),
///     year: Some(2023),
///     artist: "Artist Name".to_string(),
///     cover_art: Some(PathBuf::from("/path/to/cover.jpg")),
///     format: Some("FLAC".to_string()),
///     bit_depth: Some(24),
///     sample_rate: Some(96000),
///     dr_value: Some(12),
///     dr_is_best: true,
///     original_release_date: Some("2023-01-01".to_string()),
/// };
/// ```
#[derive(Clone)]
pub struct AlbumDisplayInfoWithYear {
    /// Unique identifier for the album in the database
    pub id: i64,
    /// Title of the album
    pub title: String,
    /// Release year of the album (may be None if unknown)
    pub year: Option<i32>,
    /// Name of the artist who created the album
    pub artist: String,
    /// Path to the album's cover art image, if available
    pub cover_art: Option<PathBuf>,
    /// Audio format of the tracks (e.g., "FLAC", "MP3", "WAV")
    pub format: Option<String>,
    /// Bit depth of the audio files in bits (e.g., 16, 24)
    pub bit_depth: Option<u32>,
    /// Sample rate of the audio files in Hz (e.g., 44100, 96000)
    pub sample_rate: Option<u32>,
    /// Dynamic Range value for the album (lower values indicate more compression)
    pub dr_value: Option<u8>,
    /// Flag indicating whether DR analysis has been marked as the best for this album
    pub dr_is_best: bool,
    /// Original release date of the album in ISO format (YYYY-MM-DD)
    pub original_release_date: Option<String>,
}

/// Fetches all albums by a given artist with display information.
///
/// This function retrieves all albums associated with a specific artist from
/// the database, along with relevant information needed for display on the
/// artist page. For each album, it includes the album metadata, artist name,
/// cover art path, and technical information from the tracks.
///
/// The function performs a database query that:
/// - Joins the albums, artists, and tracks tables
/// - Groups results by album ID to avoid duplicates
/// - Orders albums by year (descending) and title (case-insensitive)
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite database connection pool
/// * `artist_id` - The unique identifier of the artist whose albums to fetch
///
/// # Returns
///
/// Returns a `Result` containing:
/// - `Ok(Vec<AlbumDisplayInfoWithYear>)` - A vector of albums with display information
/// - `Err(sqlx::Error)` - If a database error occurs
///
/// # Examples
///
/// ```rust
/// # async fn example() -> Result<(), sqlx::Error> {
/// # use sqlx::SqlitePool;
/// # let pool: SqlitePool = todo!();
/// # let artist_id = 1;
/// let albums = fetch_album_display_info_by_artist(&pool, artist_id).await?;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_album_display_info_by_artist(
    pool: &SqlitePool,
    artist_id: i64,
) -> Result<Vec<AlbumDisplayInfoWithYear>, Error> {
    let rows = query(
        r#"SELECT albums.id, albums.title, albums.year, artists.name as artist, albums.cover_art,
                     tracks.format, tracks.bit_depth, tracks.sample_rate, albums.dr_value, albums.dr_is_best, albums.original_release_date
           FROM albums
           JOIN artists ON albums.artist_id = artists.id
           LEFT JOIN tracks ON tracks.album_id = albums.id
           WHERE albums.artist_id = ?
           GROUP BY albums.id
           ORDER BY albums.year DESC, albums.title COLLATE NOCASE"#,
   )
   .bind(artist_id)
   .fetch_all(pool)
   .await?;
    Ok(rows
        .into_iter()
        .map(|row| AlbumDisplayInfoWithYear {
            id: row.get("id"),
            title: row.get("title"),
            year: row.get("year"),
            artist: row.get("artist"),
            cover_art: row.get::<Option<String>, _>("cover_art").map(PathBuf::from),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            sample_rate: row.get("sample_rate"),
            dr_value: row.get("dr_value"),
            dr_is_best: row.get("dr_is_best"),
            original_release_date: row.get("original_release_date"),
        })
        .collect())
}

use sqlx::{Result, Row, SqlitePool, query, sqlite::SqliteRow};

use crate::data::models::{Artist, Folder};

/// Represents comprehensive album information for display in the UI.
/// This struct combines data from the `albums`, `artists`, `tracks`, and `folders` tables.
#[derive(Clone, Debug)]
pub struct AlbumDisplayInfo {
    /// Unique identifier for the album.
    pub id: i64,
    /// The title of the album.
    pub title: String,
    /// The name of the album artist.
    pub artist: String,
    /// The release year of the album (optional).
    pub year: Option<i32>,
    /// The path to the album's cached cover art image file (optional).
    pub cover_art: Option<String>,
    /// The audio format of the tracks in the album (e.g., "FLAC", "MP3", optional).
    pub format: Option<String>,
    /// The bit depth of the tracks (e.g., 16, 24, optional).
    pub bit_depth: Option<u32>,
    /// The sample rate frequency of the tracks (e.g., 44100, 96000, optional).
    pub frequency: Option<u32>,
    /// The calculated Dynamic Range (DR) value for the album (optional).
    pub dr_value: Option<u8>,
    /// Indicates whether the DR value for this album has been manually marked as completed/verified.
    pub dr_completed: bool,
    /// The original release date of the album as a string (optional).
    pub original_release_date: Option<String>,
    /// The file system path of the folder containing the album.
    pub folder_path: String,
}

/// Fetches all albums from the database along with their associated artist,
/// track format details, and folder path, suitable for display in the UI.
///
/// This function performs a JOIN operation across `albums`, `artists`, `tracks`,
/// and `folders` tables to gather all necessary information. It groups by album ID
/// to avoid duplicate album entries if an album has multiple tracks.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` containing a `Vec<AlbumDisplayInfo>` on success, or an `sqlx::Error` on failure.
pub async fn fetch_album_display_info(pool: &SqlitePool) -> Result<Vec<AlbumDisplayInfo>> {
    let rows = query(
        r#"
        SELECT
            albums.id,
            albums.title,
            artists.name AS artist,
            albums.year,
            albums.cover_art,
            tracks.format,
            tracks.bit_depth,
            tracks.frequency,
            albums.dr_value,
            albums.dr_completed,
            albums.original_release_date,
            folders.path AS folder_path
        FROM albums
        JOIN artists ON albums.artist_id = artists.id
        LEFT JOIN tracks ON tracks.album_id = albums.id -- LEFT JOIN to include albums without tracks
        JOIN folders ON albums.folder_id = folders.id
        GROUP BY albums.id
        ORDER BY artists.name COLLATE NOCASE, albums.title COLLATE NOCASE
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(map_row_to_album_display_info)
        .collect())
}

/// Helper function to map a SQLX Row to an AlbumDisplayInfo struct.
fn map_row_to_album_display_info(row: SqliteRow) -> AlbumDisplayInfo {
    AlbumDisplayInfo {
        id: row.get("id"),
        title: row.get("title"),
        artist: row.get("artist"),
        year: row.get("year"),
        cover_art: row.get("cover_art"),
        format: row.get("format"),
        bit_depth: row.get("bit_depth"),
        frequency: row.get("frequency"),
        dr_value: row.get("dr_value"),
        dr_completed: row.get("dr_completed"),
        original_release_date: row.get("original_release_date"),
        folder_path: row.get("folder_path"),
    }
}

/// Fetches all artists from the database that are associated with at least one album.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` containing a `Vec<Artist>` on success, or an `sqlx::Error` on failure.
pub async fn fetch_all_artists(pool: &SqlitePool) -> Result<Vec<Artist>> {
    let rows =
        query("SELECT id, name FROM artists WHERE id IN (SELECT DISTINCT artist_id FROM albums)")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|row| Artist {
            id: row.get("id"),
            name: row.get("name"),
        })
        .collect())
}

/// Fetches all folders stored in the database.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` containing a `Vec<Folder>` on success, or an `sqlx::Error` on failure.
pub async fn fetch_all_folders(pool: &SqlitePool) -> Result<Vec<Folder>> {
    let rows = query("SELECT id, path FROM folders")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| Folder {
            id: row.get("id"),
            path: row.get("path"),
        })
        .collect())
}

/// Searches for albums by matching a substring in their title or artist name (case-insensitive).
///
/// Returns `AlbumDisplayInfo` which combines data from `albums`, `artists`, `tracks`, and `folders`.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `search_term` - The string to search for within album titles or artist names.
///
/// # Returns
/// A `Result` containing a `Vec<AlbumDisplayInfo>` of matching albums on success,
/// or an `sqlx::Error` on failure.
pub async fn search_album_display_info(
    pool: &SqlitePool,
    search_term: &str,
) -> Result<Vec<AlbumDisplayInfo>> {
    let pattern = format!("%{}%", search_term.to_lowercase());
    let rows = query(
        r#"
        SELECT
            albums.id,
            albums.title,
            artists.name AS artist,
            albums.year,
            albums.cover_art,
            tracks.format,
            tracks.bit_depth,
            tracks.frequency,
            albums.dr_value,
            albums.dr_completed,
            albums.original_release_date,
            folders.path AS folder_path
        FROM albums
        JOIN artists ON albums.artist_id = artists.id
        LEFT JOIN tracks ON tracks.album_id = albums.id
        JOIN folders ON albums.folder_id = folders.id
        WHERE lower(albums.title) LIKE ? OR lower(artists.name) LIKE ?
        GROUP BY albums.id
        ORDER BY artists.name COLLATE NOCASE, albums.title COLLATE NOCASE
        "#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(map_row_to_album_display_info)
        .collect())
}

/// Searches for artists by matching a substring in their name (case-insensitive).
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `search_term` - The string to search for within artist names.
///
/// # Returns
/// A `Result` containing a `Vec<Artist>` of matching artists on success,
/// or an `sqlx::Error` on failure.
pub async fn search_artists(pool: &SqlitePool, search_term: &str) -> Result<Vec<Artist>> {
    let pattern = format!("%{}%", search_term.to_lowercase());
    let rows = query("SELECT id, name FROM artists WHERE lower(name) LIKE ? AND id IN (SELECT DISTINCT artist_id FROM albums)")
        .bind(&pattern)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| Artist {
            id: row.get("id"),
            name: row.get("name"),
        })
        .collect())
}

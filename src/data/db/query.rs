use std::path::PathBuf;

use sqlx::{Result, Row, SqlitePool, query, sqlite::SqliteRow};

use crate::{
    data::models::{Artist, Folder},
    ui::grids::album_grid_state::AlbumGridItem,
    utils::metadata_cache::ALBUM_DISPLAY_CACHE,
};

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
/// A `Result` containing a `Vec<AlbumGridItem>` on success, or an `sqlx::Error` on failure.
pub async fn fetch_album_display_info(pool: &SqlitePool) -> Result<Vec<AlbumGridItem>> {
    const CACHE_KEY: &str = "album_display_info";

    // Try to get from cache first
    if let Some(cached) = ALBUM_DISPLAY_CACHE.get(CACHE_KEY) {
        return Ok(cached);
    }

    // Fetch album display information from database when not cached
    let result = fetch_album_display_info_from_db(pool).await?;
    
    // Cache the result
    ALBUM_DISPLAY_CACHE.insert(CACHE_KEY.to_string(), result.clone());
    Ok(result)
}

/// Helper function to fetch album display information from the database
async fn fetch_album_display_info_from_db(pool: &SqlitePool) -> Result<Vec<AlbumGridItem>> {
    // Execute the query to fetch all album display information
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
    Ok(rows.into_iter().map(map_row_to_album_grid_item).collect())
}

/// Helper function to map a SQLX Row to an AlbumGridItem struct.
fn map_row_to_album_grid_item(row: SqliteRow) -> AlbumGridItem {
    AlbumGridItem {
        id: row.get("id"),
        title: row.get("title"),
        artist: row.get("artist"),
        year: row.get("year"),
        cover_art: row.get("cover_art"),
        format: row.get("format"),
        bit_depth: row.get::<Option<u32>, _>("bit_depth").map(|bd| bd as i32),
        frequency: row.get::<Option<u32>, _>("frequency").map(|f| f as i32),
        dr_value: row.get::<Option<u8>, _>("dr_value").map(|dr| dr as i32),
        dr_completed: row.get("dr_completed"),
        original_release_date: row.get("original_release_date"),
        folder_path: PathBuf::from(row.get::<String, _>("folder_path")),
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
            path: PathBuf::from(row.get::<String, _>("path")),
        })
        .collect())
}

/// Searches for albums by matching a substring in their title or artist name (case-insensitive).
///
/// Returns `AlbumGridItem` which combines data from `albums`, `artists`, `tracks`, and `folders`.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `search_term` - The string to search for within album titles or artist names.
///
/// # Returns
/// A `Result` containing a `Vec<AlbumGridItem>` of matching albums on success,
/// or an `sqlx::Error` on failure.
pub async fn search_album_display_info(
    pool: &SqlitePool,
    search_term: &str,
) -> Result<Vec<AlbumGridItem>> {
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
    Ok(rows.into_iter().map(map_row_to_album_grid_item).collect())
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

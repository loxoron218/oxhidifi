use std::path::PathBuf;

use sqlx::{Result, Row, SqlitePool, query};

use crate::{
    data::models::{Artist, Folder},
    ui::grids::album_grid_state::AlbumGridItem,
};

/// Helper function to map a SQLX Row to an AlbumGridItem struct.
fn map_row_to_album_grid_item(row: sqlx::sqlite::SqliteRow) -> AlbumGridItem {
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

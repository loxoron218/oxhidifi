use std::path::Path;

use sqlx::{Result, Row, SqlitePool, query};

/// Removes a folder and all associated albums and tracks from the database.
/// Also removes any artists that become orphaned (no remaining albums or tracks)
/// after the folder's content is deleted.
///
/// This operation is performed within a transaction to ensure atomicity.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `folder_id` - The ID of the folder to be removed.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_folder_and_albums(pool: &SqlitePool, folder_id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Remove tracks belonging to albums within the specified folder
    query("DELETE FROM tracks WHERE album_id IN (SELECT id FROM albums WHERE folder_id = ?)")
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;

    // Remove albums associated with the specified folder
    query("DELETE FROM albums WHERE folder_id = ?")
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;

    // Clean up artists who no longer have any associated albums or tracks
    query("DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM albums) AND id NOT IN (SELECT artist_id FROM tracks)")
        .execute(&mut *tx)
        .await?;

    // Finally, remove the folder itself
    query("DELETE FROM folders WHERE id = ?")
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Removes an album and all its associated tracks from the database by album ID.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `album_id` - The ID of the album to be removed.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_album_and_tracks(pool: &SqlitePool, album_id: i64) -> Result<()> {
    // Remove all tracks associated with the specified album
    query("DELETE FROM tracks WHERE album_id = ?")
        .bind(album_id)
        .execute(pool)
        .await?;

    // Remove the album itself
    query("DELETE FROM albums WHERE id = ?")
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes artists from the database who are no longer associated with any albums or tracks.
/// This helps in cleaning up orphaned artist entries.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_artists_with_no_albums(pool: &SqlitePool) -> Result<()> {
    // Remove artists who are not associated with any albums or tracks
    query("DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM albums) AND id NOT IN (SELECT artist_id FROM tracks)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes albums from the database that no longer have any associated tracks.
/// This helps in cleaning up orphaned album entries.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_albums_with_no_tracks(pool: &SqlitePool) -> Result<()> {
    // Remove albums that don't have any associated tracks
    query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes track entries from the database whose corresponding files no longer exist on disk.
/// This function iterates through all tracks in the database and checks their file paths.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_orphaned_tracks(pool: &SqlitePool) -> Result<()> {
    let tracks_in_db = query("SELECT id, path FROM tracks").fetch_all(pool).await?;
    for track_row in tracks_in_db {
        let track_id: i64 = track_row.get("id");
        let track_path: String = track_row.get("path");
        if !Path::new(&track_path).exists() {
            // Remove the track from the database since its file no longer exists
            query("DELETE FROM tracks WHERE id = ?")
                .bind(track_id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

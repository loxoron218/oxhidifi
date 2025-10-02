use std::path::Path;

use sqlx::{Result, Row, SqlitePool, query};

/// Removes a folder and all associated albums and songs from the database.
/// Also removes any artists that become orphaned (no remaining albums or songs)
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

    // Remove songs belonging to albums within the specified folder
    query("DELETE FROM songs WHERE album_id IN (SELECT id FROM albums WHERE folder_id = ?)")
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;

    // Remove albums associated with the specified folder
    query("DELETE FROM albums WHERE folder_id = ?")
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;

    // Clean up artists who no longer have any associated albums or songs
    query("DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM albums) AND id NOT IN (SELECT artist_id FROM songs)")
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

/// Removes an album and all its associated songs from the database by album ID.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `album_id` - The ID of the album to be removed.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_album_and_songs(pool: &SqlitePool, album_id: i64) -> Result<()> {
    // Remove all songs associated with the specified album
    query("DELETE FROM songs WHERE album_id = ?")
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

/// Removes artists from the database who are no longer associated with any albums or songs.
/// This helps in cleaning up orphaned artist entries.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_artists_with_no_albums(pool: &SqlitePool) -> Result<()> {
    // Remove artists who are not associated with any albums or songs
    query("DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM albums) AND id NOT IN (SELECT artist_id FROM songs)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes albums from the database that no longer have any associated songs.
/// This helps in cleaning up orphaned album entries.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_albums_with_no_songs(pool: &SqlitePool) -> Result<()> {
    // Remove albums that don't have any associated songs
    query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM songs)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes song entries from the database whose corresponding files no longer exist on disk.
/// This function iterates through all songs in the database and checks their file paths.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn remove_orphaned_songs(pool: &SqlitePool) -> Result<()> {
    let songs_in_db = query("SELECT id, path FROM songs").fetch_all(pool).await?;

    // Process each song to check if its file still exists
    for song_row in songs_in_db {
        let song_id: i64 = song_row.get("id");
        let song_path: String = song_row.get("path");

        // Skip to next song if file still exists
        if Path::new(&song_path).exists() {
            continue;
        }

        // Remove the song from the database since its file no longer exists
        query("DELETE FROM songs WHERE id = ?")
            .bind(song_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

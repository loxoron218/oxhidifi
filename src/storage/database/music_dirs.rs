//! Configured library directory management.

use std::path::Path;

use sqlx::{query, query_as};

use crate::storage::{
    StorageError::{Database, InvalidPath},
    StorageResult,
    catalog::LibraryDirectory,
    database::SqliteStorage,
};

impl SqliteStorage {
    /// List all configured library directories, ordered by path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn list_library_directory_rows(&self) -> StorageResult<Vec<LibraryDirectory>> {
        query_as::<_, LibraryDirectory>("SELECT * FROM library_directories ORDER BY path")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("List directories failed: {e}")))
    }

    /// Add a library directory row, ignoring duplicates.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidPath`] if the path is not valid UTF-8,
    /// or [`StorageError::Database`] if the query fails.
    pub async fn add_library_directory_row(&self, path: &Path) -> StorageResult<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| InvalidPath(path.display().to_string()))?;

        _ = query("INSERT OR IGNORE INTO library_directories (path) VALUES (?)")
            .bind(path_str)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Add directory failed: {e}")))?;

        Ok(())
    }

    /// Remove a library directory row by ID and hard-delete all tracks
    /// whose `file_path` is under that directory, plus orphan albums/artists.
    ///
    /// Matches `file_path == dir` OR `file_path LIKE dir || '/%'` with
    /// trailing-slash normalization so `/music` does not match `/music2`.
    /// Orphan `albums`/`artists` are hard-deleted immediately (no ghosts with
    /// `track_count=0`). Runs in a single transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any query or the transaction fails.
    pub async fn remove_library_directory_row(&self, id: i64) -> StorageResult<()> {
        let path_row: Option<(String,)> =
            query_as("SELECT path FROM library_directories WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| Database(format!("Fetch directory path failed: {e}")))?;

        let Some((dir,)) = path_row else {
            return Ok(());
        };

        let dir_norm = dir.trim_end_matches('/').to_string();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Database(format!("Begin transaction failed: {e}")))?;

        _ = query("DELETE FROM tracks WHERE file_path = ? OR file_path LIKE ? || '/%'")
            .bind(&dir_norm)
            .bind(&dir_norm)
            .execute(&mut *tx)
            .await
            .map_err(|e| Database(format!("Delete tracks by directory failed: {e}")))?;

        _ = query(
            "DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks WHERE \
             album_id IS NOT NULL)",
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| Database(format!("Delete orphan albums failed: {e}")))?;

        _ = query(
            "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums) AND id \
             NOT IN (SELECT DISTINCT artist_id FROM tracks WHERE artist_id IS NOT NULL)",
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| Database(format!("Delete orphan artists failed: {e}")))?;

        _ = query("DELETE FROM library_directories WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Database(format!("Remove directory failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| Database(format!("Commit transaction failed: {e}")))?;

        Ok(())
    }

    /// Delete orphan albums and artists (no remaining tracks/albums).
    ///
    /// Used by the file watcher after deleting a single track on removal.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any delete fails.
    pub async fn prune_orphans_row(&self) -> StorageResult<()> {
        _ = query(
            "DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks WHERE \
             album_id IS NOT NULL)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Database(format!("Delete orphan albums failed: {e}")))?;

        _ = query(
            "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums) AND id \
             NOT IN (SELECT DISTINCT artist_id FROM tracks WHERE artist_id IS NOT NULL)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Database(format!("Delete orphan artists failed: {e}")))?;

        Ok(())
    }
}

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

        query("INSERT OR IGNORE INTO library_directories (path) VALUES (?)")
            .bind(path_str)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Add directory failed: {e}")))?;

        Ok(())
    }

    /// Remove a library directory row by ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn remove_library_directory_row(&self, id: i64) -> StorageResult<()> {
        query("DELETE FROM library_directories WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Remove directory failed: {e}")))?;

        Ok(())
    }
}

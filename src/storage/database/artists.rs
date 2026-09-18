//! Artist row operations.

use sqlx::query_as;

use crate::storage::{
    StorageError::{self, Database},
    catalog::{Artist, NewArtist},
    database::SqliteStorage,
};

impl SqliteStorage {
    /// Insert a new artist row and return its ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the insert query fails.
    pub async fn insert_artist_row(&self, artist: &NewArtist) -> Result<i64, StorageError> {
        let row_id: (i64,) = query_as("INSERT INTO artists (name) VALUES (?) RETURNING id")
            .bind(&artist.name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Database(format!("Insert artist failed: {e}")))?;

        Ok(row_id.0)
    }

    /// Fetch a single artist row by ID, including album count.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn get_artist_row(&self, id: i64) -> Result<Option<Artist>, StorageError> {
        query_as::<_, Artist>(
            "SELECT ar.id, ar.name, (SELECT COUNT(*) FROM albums WHERE artist_id = ar.id) AS \
             album_count FROM artists ar WHERE ar.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Database(format!("Get artist failed: {e}")))
    }

    /// Fetch all artist rows, ordered by name.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn all_artists_rows(&self) -> Result<Vec<Artist>, StorageError> {
        query_as::<_, Artist>(
            "SELECT ar.id, ar.name, (SELECT COUNT(*) FROM albums WHERE artist_id = ar.id) AS \
             album_count FROM artists ar ORDER BY ar.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Get all artists failed: {e}")))
    }
}

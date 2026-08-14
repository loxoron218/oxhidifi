//! Playback queue table operations.

use sqlx::{query, query_as};

use crate::storage::{
    StorageError::Database,
    StorageResult,
    catalog::{
        NewQueueEntry,
        QueueContext::{self, Album as QueueAlbum, Artist as QueueArtist, Manual},
        QueueEntry,
    },
    database::SqliteStorage,
};

impl SqliteStorage {
    /// Fetch the entire playback queue, ordered by position.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn get_queue_rows(&self) -> StorageResult<Vec<QueueEntry>> {
        query_as::<_, QueueEntry>("SELECT * FROM playback_queue ORDER BY position")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get queue failed: {e}")))
    }

    /// Replace the entire playback queue with the given entries.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any query fails.
    pub async fn set_queue_rows(&self, entries: &[NewQueueEntry]) -> StorageResult<()> {
        query("DELETE FROM playback_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Clear queue failed: {e}")))?;

        for entry in entries {
            query(
                "INSERT INTO playback_queue (track_id, position, context_type, context_id) VALUES \
                 (?, ?, ?, ?)",
            )
            .bind(entry.track_id)
            .bind(entry.position)
            .bind(&entry.context_type)
            .bind(entry.context_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Set queue entry failed: {e}")))?;
        }

        Ok(())
    }

    /// Append a track to the end of the playback queue.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any query fails.
    pub async fn append_queue_row(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> StorageResult<()> {
        let max_pos: Option<(i32,)> =
            query_as("SELECT COALESCE(MAX(position), -1) FROM playback_queue")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| Database(format!("Queue max failed: {e}")))?;

        let next_pos = max_pos.map_or(0, |(p,)| p.saturating_add(1));

        let (context_type, context_id) = match context {
            Some(QueueAlbum(id)) => (Some("album".to_string()), Some(id)),
            Some(QueueArtist(id)) => (Some("artist".to_string()), Some(id)),
            Some(Manual) | None => (None, None),
        };

        query(
            "INSERT INTO playback_queue (track_id, position, context_type, context_id) VALUES (?, \
             ?, ?, ?)",
        )
        .bind(track_id)
        .bind(next_pos)
        .bind(context_type)
        .bind(context_id)
        .execute(&self.pool)
        .await
        .map_err(|e| Database(format!("Append queue failed: {e}")))?;

        Ok(())
    }

    /// Remove a playback queue entry by ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn remove_queue_entry_row(&self, id: i64) -> StorageResult<()> {
        query("DELETE FROM playback_queue WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Remove queue entry failed: {e}")))?;

        Ok(())
    }

    /// Move a playback queue entry to a new position.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn reorder_queue_row(&self, entry_id: i64, new_position: u32) -> StorageResult<()> {
        query("UPDATE playback_queue SET position = ? WHERE id = ?")
            .bind(new_position.cast_signed())
            .bind(entry_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Reorder queue failed: {e}")))?;

        Ok(())
    }

    /// Clear the entire playback queue.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn clear_queue_rows(&self) -> StorageResult<()> {
        query("DELETE FROM playback_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Clear queue failed: {e}")))?;

        Ok(())
    }
}

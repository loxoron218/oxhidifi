//! Playback queue table operations.

use sqlx::{query, query_as};

use crate::storage::{
    StorageError::{Database, QueueFull},
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
        _ = query("DELETE FROM playback_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Clear queue failed: {e}")))?;

        for entry in entries {
            _ = query(
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
    /// Returns [`StorageError::QueueFull`] if the queue already contains 100,000 entries,
    /// or [`StorageError::Database`] if any query fails.
    pub async fn append_queue_row(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> StorageResult<()> {
        let count: (i64,) = query_as("SELECT COUNT(*) FROM playback_queue")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Database(format!("Queue count failed: {e}")))?;
        if count.0 >= 100_000 {
            return Err(QueueFull { max: 100_000 });
        }
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

        _ = query(
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
        _ = query("DELETE FROM playback_queue WHERE id = ?")
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
        _ = query("UPDATE playback_queue SET position = ? WHERE id = ?")
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
        _ = query("DELETE FROM playback_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Clear queue failed: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Context, Result, ensure},
        tempfile::tempdir,
        tokio::test,
    };

    use crate::storage::{
        catalog::{NewQueueEntry, NewTrack, QueueContext, TrackAudio},
        database::tests::storage_in,
    };

    fn make_track(title: &str, path: &str) -> NewTrack {
        NewTrack {
            title: title.to_string(),
            track_number: Some(1),
            disc_number: Some(1),
            duration: 180.0,
            audio: TrackAudio {
                file_path: path.to_string(),
                content_hash: None,
                format: "FLAC".to_string(),
                sample_rate: 44100,
                bit_depth: Some(16),
                channels: 2,
                codec: "flac".to_string(),
                lossless: true,
                bitrate: None,
                album_id: None,
                artist_id: None,
                file_size: 1024,
                last_modified: "2024-01-01T00:00:00Z".to_string(),
            },
        }
    }

    #[test]
    async fn queue_rows_round_trip_through_storage() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;
        let track_a = storage
            .insert_track_row(&make_track("Queue A", "/music/queue_a.flac"))
            .await?;
        let track_b = storage
            .insert_track_row(&make_track("Queue B", "/music/queue_b.flac"))
            .await?;

        let initial = storage.get_queue_rows().await?;
        ensure!(initial.is_empty(), "queue should start empty");

        storage
            .set_queue_rows(&[
                NewQueueEntry {
                    track_id: track_a,
                    position: 0,
                    context_type: Some("album".to_string()),
                    context_id: Some(7),
                },
                NewQueueEntry {
                    track_id: track_b,
                    position: 1,
                    context_type: None,
                    context_id: None,
                },
            ])
            .await?;
        let rows = storage.get_queue_rows().await?;
        ensure!(rows.len() == 2, "expected 2 queue entries");
        let first = rows.first().context("queue must have a first entry")?;
        ensure!(first.track_id == track_a, "first entry must match");
        ensure!(
            first.context_type.as_deref() == Some("album"),
            "context type must round-trip"
        );
        ensure!(
            format!("{first:?}").contains(&track_a.to_string()),
            "queue entry debug must include track id"
        );

        storage
            .append_queue_row(track_a, Some(QueueContext::Album(7)))
            .await?;
        storage
            .append_queue_row(track_b, Some(QueueContext::Artist(9)))
            .await?;
        storage
            .append_queue_row(track_a, Some(QueueContext::Manual))
            .await?;
        storage.append_queue_row(track_b, None).await?;
        let rows = storage.get_queue_rows().await?;
        ensure!(rows.len() == 6, "expected 6 queue entries");

        let first_id = rows.first().context("queue must have a first entry")?.id;
        storage.reorder_queue_row(first_id, 5).await?;
        storage.remove_queue_entry_row(first_id).await?;
        let rows = storage.get_queue_rows().await?;
        ensure!(rows.len() == 5, "expected 5 entries after remove");

        storage.clear_queue_rows().await?;
        let rows = storage.get_queue_rows().await?;
        ensure!(rows.is_empty(), "queue should be empty after clear");
        Ok(())
    }
}

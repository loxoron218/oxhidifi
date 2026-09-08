//! Track row operations: insert, update, delete, path/hash/fingerprint lookups,
//! and bulk insert/batch look-up.

use std::path::Path;

use sqlx::{QueryBuilder, query, query_as};

use crate::storage::{
    StorageError::{Database, InvalidPath},
    StorageResult,
    catalog::{
        FieldUpdate::{Set, SetNull, Skip},
        NewTrack, Track, TrackUpdate,
    },
    database::SqliteStorage,
};

/// Apply a `FieldUpdate` to a column in the tracks table.
macro_rules! apply_field {
    ($track:expr, $field:ident, $pool:expr, $id:expr) => {
        match &$track.$field {
            Set(v) => {
                _ = query(concat!(
                    "UPDATE tracks SET ",
                    stringify!($field),
                    " = ? WHERE id = ?"
                ))
                .bind(v)
                .bind($id)
                .execute(&$pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
            }
            SetNull => {
                _ = query(concat!(
                    "UPDATE tracks SET ",
                    stringify!($field),
                    " = NULL WHERE id = ?"
                ))
                .bind($id)
                .execute(&$pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
            }
            Skip => {}
        }
    };
}

impl SqliteStorage {
    /// Insert a new track row into the database and return its ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the insert query fails.
    pub async fn insert_track_row(&self, track: &NewTrack) -> StorageResult<i64> {
        let row_id: (i64,) = query_as(
            "INSERT INTO tracks (title, number, disc_number, duration, file_path, content_hash, \
             format, sample_rate, bit_depth, channels, codec, lossless, bitrate, album_id, \
             artist_id, file_size, last_modified) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
             ?, ?, ?, ?) RETURNING id",
        )
        .bind(&track.title)
        .bind(track.track_number)
        .bind(track.disc_number)
        .bind(track.duration)
        .bind(&track.audio.file_path)
        .bind(&track.audio.content_hash)
        .bind(&track.audio.format)
        .bind(track.audio.sample_rate)
        .bind(track.audio.bit_depth)
        .bind(track.audio.channels)
        .bind(&track.audio.codec)
        .bind(track.audio.lossless)
        .bind(track.audio.bitrate)
        .bind(track.audio.album_id)
        .bind(track.audio.artist_id)
        .bind(track.audio.file_size)
        .bind(&track.audio.last_modified)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Database(format!("Insert track failed: {e}")))?;

        Ok(row_id.0)
    }

    /// Apply a `TrackUpdate` to an existing track row.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any update query fails.
    pub async fn update_track_row(&self, id: i64, track: TrackUpdate) -> StorageResult<()> {
        if let Some(title) = track.title {
            _ = query("UPDATE tracks SET title = ? WHERE id = ?")
                .bind(&title)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
        }
        apply_field!(track, track_number, self.pool, id);
        apply_field!(track, disc_number, self.pool, id);
        if let Some(duration) = track.duration {
            _ = query("UPDATE tracks SET duration = ? WHERE id = ?")
                .bind(duration)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
        }
        apply_field!(track, content_hash, self.pool, id);
        apply_field!(track, album_id, self.pool, id);
        apply_field!(track, artist_id, self.pool, id);
        Ok(())
    }

    /// Delete a track row by ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the delete query fails.
    pub async fn delete_track_row(&self, id: i64) -> StorageResult<()> {
        _ = query("DELETE FROM tracks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Delete track failed: {e}")))?;
        Ok(())
    }

    /// Fetch a track row by ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn get_track_row(&self, id: i64) -> StorageResult<Option<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Database(format!("Get track failed: {e}")))
    }

    /// Fetch all track rows belonging to an album, ordered by track number.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn tracks_by_album(&self, album_id: i64) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE album_id = ? ORDER BY number")
            .bind(album_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by album failed: {e}")))
    }

    /// Fetch all track rows belonging to an artist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn tracks_by_artist(&self, artist_id: i64) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE artist_id = ?")
            .bind(artist_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by artist failed: {e}")))
    }

    /// Search track rows by title or file path substring.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn search_track_rows(&self, search: &str) -> StorageResult<Vec<Track>> {
        let pattern = format!("%{search}%");
        query_as::<_, Track>("SELECT * FROM tracks WHERE title LIKE ? OR file_path LIKE ?")
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Search tracks failed: {e}")))
    }

    /// Find a single track row by exact file path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidPath`] if the path is not valid UTF-8,
    /// or [`StorageError::Database`] if the query fails.
    pub async fn find_by_path_row(&self, path: &Path) -> StorageResult<Option<Track>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| InvalidPath(path.display().to_string()))?;

        query_as::<_, Track>("SELECT * FROM tracks WHERE file_path = ?")
            .bind(path_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Database(format!("Find by path failed: {e}")))
    }

    /// Find all track rows with a matching content hash.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn find_by_hash_rows(&self, hash: &str) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE content_hash = ?")
            .bind(hash)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Find by hash failed: {e}")))
    }

    /// Check if any track exists with the given content hash (exists-only).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn hash_exists_row(&self, hash: &str) -> StorageResult<bool> {
        let exists: (i64,) = query_as("SELECT EXISTS(SELECT 1 FROM tracks WHERE content_hash = ?)")
            .bind(hash)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Database(format!("Hash exists check failed: {e}")))?;
        Ok(exists.0 != 0)
    }

    /// Find track rows by artist/album/title metadata fingerprint.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn find_by_fingerprint_rows(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>(
            "SELECT t.* FROM tracks t JOIN albums a ON t.album_id = a.id JOIN artists ar ON \
             t.artist_id = ar.id WHERE ar.name = ? AND a.title = ? AND t.title = ? AND (? IS NULL \
             OR t.number = ?)",
        )
        .bind(artist)
        .bind(album)
        .bind(title)
        .bind(track.map(u32::cast_signed))
        .bind(track.map(u32::cast_signed))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Find by fingerprint failed: {e}")))
    }

    /// Insert multiple track rows in a batch, returning their IDs.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any insert query fails.
    pub async fn insert_tracks_batch_rows(&self, tracks: Vec<NewTrack>) -> StorageResult<Vec<i64>> {
        let mut ids = Vec::with_capacity(tracks.len());
        for track in &tracks {
            ids.push(self.insert_track_row(track).await?);
        }
        Ok(ids)
    }

    /// Find track rows by multiple file paths in a batch.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidPath`] for a non-UTF-8 path, or
    /// [`StorageError::Database`] if any query fails.
    pub async fn find_by_paths_batch_rows(
        &self,
        paths: &[&Path],
    ) -> StorageResult<Vec<Option<Track>>> {
        let mut results = Vec::with_capacity(paths.len());
        for path in paths {
            let path_str = path
                .to_str()
                .ok_or_else(|| InvalidPath(path.display().to_string()))?;

            let track = query_as::<_, Track>("SELECT * FROM tracks WHERE file_path = ?")
                .bind(path_str)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| Database(format!("Find by path failed: {e}")))?;
            results.push(track);
        }
        Ok(results)
    }

    /// Find track rows by multiple content hashes in a batch.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if any query fails.
    pub async fn find_by_hashes_batch_rows(
        &self,
        hashes: &[&str],
    ) -> StorageResult<Vec<Vec<Track>>> {
        let mut results = Vec::with_capacity(hashes.len());
        for hash in hashes {
            let tracks = query_as::<_, Track>("SELECT * FROM tracks WHERE content_hash = ?")
                .bind(hash)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Database(format!("Find by hash failed: {e}")))?;
            results.push(tracks);
        }
        Ok(results)
    }

    /// Fetch track rows belonging to multiple albums in a single query.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn tracks_by_albums_rows(&self, album_ids: &[i64]) -> StorageResult<Vec<Track>> {
        if album_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::new("SELECT * FROM tracks WHERE album_id IN (");
        let mut separated = builder.separated(", ");
        for id in album_ids {
            _ = separated.push_bind(id);
        }
        _ = builder.push(") ORDER BY album_id, number");
        builder
            .build_query_as::<Track>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by albums failed: {e}")))
    }

    /// Fetch multiple track rows by their IDs in a single query.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn tracks_by_ids_rows(&self, ids: &[i64]) -> StorageResult<Vec<Track>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::new("SELECT * FROM tracks WHERE id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            _ = separated.push_bind(id);
        }
        _ = builder.push(")");
        builder
            .build_query_as::<Track>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by ids failed: {e}")))
    }
}

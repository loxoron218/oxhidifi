//! [`Storage`] trait implementation for [`SqliteStorage`].
//!
//! Thin delegates to the per-domain row helpers in the sibling modules; each
//! method forwards to its `*_row` / `*_rows` counterpart.

use std::{collections::HashMap, path::Path};

use crate::storage::{
    Storage, StorageError,
    catalog::{
        Album, Artist, LibraryDirectory, NewAlbum, NewArtist, NewQueueEntry, NewTrack,
        QueueContext, QueueEntry, Track, TrackUpdate,
    },
    database::SqliteStorage,
    formats::FormatInfo,
};

impl Storage for SqliteStorage {
    /// Insert a new track, returning its id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn insert_track(&self, track: NewTrack) -> Result<i64, StorageError> {
        self.insert_track_row(&track).await
    }

    /// Update an existing track.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn update_track(&self, id: i64, track: TrackUpdate) -> Result<(), StorageError> {
        self.update_track_row(id, track).await
    }

    /// Delete a track by id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn delete_track(&self, id: i64) -> Result<(), StorageError> {
        self.delete_track_row(id).await
    }

    /// Get a track by id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_track(&self, id: i64) -> Result<Option<Track>, StorageError> {
        self.get_track_row(id).await
    }

    /// Get all tracks belonging to an album.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_tracks_by_album(&self, album_id: i64) -> Result<Vec<Track>, StorageError> {
        self.tracks_by_album(album_id).await
    }

    /// Get all tracks by an artist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_tracks_by_artist(&self, artist_id: i64) -> Result<Vec<Track>, StorageError> {
        self.tracks_by_artist(artist_id).await
    }

    /// Search tracks by query string.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn search_tracks(&self, query: &str) -> Result<Vec<Track>, StorageError> {
        self.search_track_rows(query).await
    }

    /// Insert a new album, returning its id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn insert_album(&self, album: NewAlbum) -> Result<i64, StorageError> {
        self.insert_album_row(&album).await
    }

    /// Get an album by id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_album(&self, id: i64) -> Result<Option<Album>, StorageError> {
        self.get_album_row(id).await
    }

    /// Get all albums.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_all_albums(&self) -> Result<Vec<Album>, StorageError> {
        self.all_albums_rows().await
    }

    /// Get distinct format info for a single album.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_album_format_info(&self, album_id: i64) -> Result<FormatInfo, StorageError> {
        self.album_format_info_rows(album_id).await
    }

    /// Get distinct format info for multiple albums at once.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_albums_format_info(
        &self,
        album_ids: &[i64],
    ) -> Result<HashMap<i64, FormatInfo>, StorageError> {
        self.albums_format_info_rows(album_ids).await
    }

    /// Get all albums by an artist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_albums_by_artist(&self, artist_id: i64) -> Result<Vec<Album>, StorageError> {
        self.albums_by_artist_rows(artist_id).await
    }

    /// Update an album's cached artwork path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn update_album_artwork(
        &self,
        album_id: i64,
        artwork_path: Option<String>,
    ) -> Result<(), StorageError> {
        self.update_album_artwork_row(album_id, artwork_path).await
    }

    /// Insert a new artist, returning its id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn insert_artist(&self, artist: NewArtist) -> Result<i64, StorageError> {
        self.insert_artist_row(&artist).await
    }

    /// Get an artist by id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_artist(&self, id: i64) -> Result<Option<Artist>, StorageError> {
        self.get_artist_row(id).await
    }

    /// Get all artists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_all_artists(&self) -> Result<Vec<Artist>, StorageError> {
        self.all_artists_rows().await
    }

    /// List all configured library directories.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn list_library_directories(&self) -> Result<Vec<LibraryDirectory>, StorageError> {
        self.list_library_directory_rows().await
    }

    /// Add a library directory.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn add_library_directory(&self, path: &Path) -> Result<(), StorageError> {
        self.add_library_directory_row(path).await
    }

    /// Remove a library directory by id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn remove_library_directory(&self, id: i64) -> Result<(), StorageError> {
        self.remove_library_directory_row(id).await
    }

    /// Get the current playback queue.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_queue(&self) -> Result<Vec<QueueEntry>, StorageError> {
        self.get_queue_rows().await
    }

    /// Replace the entire queue.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn set_queue(&self, entries: &[NewQueueEntry]) -> Result<(), StorageError> {
        self.set_queue_rows(entries).await
    }

    /// Append a track to the end of the queue.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] if the queue is at capacity, or
    /// [`StorageError::Database`] if the query fails.
    async fn append_queue(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> Result<(), StorageError> {
        self.append_queue_row(track_id, context).await
    }

    /// Remove a queue entry by id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn remove_queue_entry(&self, id: i64) -> Result<(), StorageError> {
        self.remove_queue_entry_row(id).await
    }

    /// Move a queue entry to a new position.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn reorder_queue(&self, entry_id: i64, new_position: u32) -> Result<(), StorageError> {
        self.reorder_queue_row(entry_id, new_position).await
    }

    /// Clear the entire queue.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn clear_queue(&self) -> Result<(), StorageError> {
        self.clear_queue_rows().await
    }

    /// Find a track by file path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn find_by_path(&self, path: &Path) -> Result<Option<Track>, StorageError> {
        self.find_by_path_row(path).await
    }

    /// Find tracks by content hash.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn find_by_hash(&self, hash: &str) -> Result<Vec<Track>, StorageError> {
        self.find_by_hash_rows(hash).await
    }

    /// Find tracks by metadata fingerprint.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn find_by_metadata_fingerprint(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> Result<Vec<Track>, StorageError> {
        self.find_by_fingerprint_rows(artist, album, title, track)
            .await
    }

    /// Insert multiple tracks in a batch, returning their ids.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn insert_tracks_batch(&self, tracks: Vec<NewTrack>) -> Result<Vec<i64>, StorageError> {
        self.insert_tracks_batch_rows(tracks).await
    }

    /// Find tracks by multiple file paths in a batch.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn find_by_paths_batch(
        &self,
        paths: &[&Path],
    ) -> Result<Vec<Option<Track>>, StorageError> {
        self.find_by_paths_batch_rows(paths).await
    }

    /// Find tracks by multiple content hashes in a batch.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn find_by_hashes_batch(&self, hashes: &[&str]) -> Result<Vec<Vec<Track>>, StorageError> {
        self.find_by_hashes_batch_rows(hashes).await
    }

    /// Get tracks belonging to multiple albums in a single query.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_tracks_by_albums(&self, album_ids: &[i64]) -> Result<Vec<Track>, StorageError> {
        self.tracks_by_albums_rows(album_ids).await
    }

    /// Get multiple tracks by their IDs in a single query.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn get_tracks_by_ids(&self, ids: &[i64]) -> Result<Vec<Track>, StorageError> {
        self.tracks_by_ids_rows(ids).await
    }

    /// Check if any track exists with the given content hash.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn hash_exists(&self, hash: &str) -> Result<bool, StorageError> {
        self.hash_exists_row(hash).await
    }

    /// Delete orphan albums and artists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the underlying query fails.
    async fn prune_orphans(&self) -> Result<(), StorageError> {
        self.prune_orphans_row().await
    }
}

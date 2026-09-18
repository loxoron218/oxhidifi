//! Persistence layer: domain types, storage trait, and error types.

pub mod active_tab;
pub mod catalog;
pub mod config;
pub mod database;
pub mod formats;
pub mod migrations;
pub mod settings;
pub mod sort_rules;
pub mod view_mode;

use std::{collections::HashMap, path::Path};

use thiserror::Error;

use crate::storage::{
    catalog::{
        Album, Artist, LibraryDirectory, NewAlbum, NewArtist, NewQueueEntry, NewTrack,
        QueueContext, QueueEntry, Track, TrackUpdate,
    },
    formats::FormatInfo,
};

/// Interface for all persistent storage operations.
pub trait Storage: Send + Sync + 'static {
    /// Insert a new track, returning its id.
    fn insert_track(
        &self,
        track: NewTrack,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Update an existing track.
    fn update_track(
        &self,
        id: i64,
        track: TrackUpdate,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Delete a track by id.
    fn delete_track(&self, id: i64) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Get a track by id.
    fn get_track(
        &self,
        id: i64,
    ) -> impl Future<Output = Result<Option<Track>, StorageError>> + Send;

    /// Get all tracks belonging to an album.
    fn get_tracks_by_album(
        &self,
        album_id: i64,
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Get all tracks by an artist.
    fn get_tracks_by_artist(
        &self,
        artist_id: i64,
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Search tracks by query string.
    fn search_tracks(
        &self,
        query: &str,
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Insert a new album, returning its id.
    fn insert_album(
        &self,
        album: NewAlbum,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Get an album by id.
    fn get_album(
        &self,
        id: i64,
    ) -> impl Future<Output = Result<Option<Album>, StorageError>> + Send;

    /// Get all albums.
    fn get_all_albums(&self) -> impl Future<Output = Result<Vec<Album>, StorageError>> + Send;

    /// Get distinct format info for a single album.
    fn get_album_format_info(
        &self,
        album_id: i64,
    ) -> impl Future<Output = Result<FormatInfo, StorageError>> + Send;

    /// Get distinct format info for multiple albums at once.
    fn get_albums_format_info(
        &self,
        album_ids: &[i64],
    ) -> impl Future<Output = Result<HashMap<i64, FormatInfo>, StorageError>> + Send;

    /// Get all albums by an artist.
    fn get_albums_by_artist(
        &self,
        artist_id: i64,
    ) -> impl Future<Output = Result<Vec<Album>, StorageError>> + Send;

    /// Insert a new artist, returning its id.
    fn insert_artist(
        &self,
        artist: NewArtist,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Get an artist by id.
    fn get_artist(
        &self,
        id: i64,
    ) -> impl Future<Output = Result<Option<Artist>, StorageError>> + Send;

    /// Get all artists.
    fn get_all_artists(&self) -> impl Future<Output = Result<Vec<Artist>, StorageError>> + Send;

    /// List all configured library directories.
    fn list_library_directories(
        &self,
    ) -> impl Future<Output = Result<Vec<LibraryDirectory>, StorageError>> + Send;

    /// Add a library directory.
    fn add_library_directory(
        &self,
        path: &Path,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Remove a library directory by id.
    fn remove_library_directory(
        &self,
        id: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Get the current playback queue.
    fn get_queue(&self) -> impl Future<Output = Result<Vec<QueueEntry>, StorageError>> + Send;

    /// Replace the entire queue.
    fn set_queue(
        &self,
        entries: &[NewQueueEntry],
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Append a track to the end of the queue.
    fn append_queue(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Remove a queue entry by id.
    fn remove_queue_entry(&self, id: i64) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Move a queue entry to a new position.
    fn reorder_queue(
        &self,
        entry_id: i64,
        new_position: u32,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Clear the entire queue.
    fn clear_queue(&self) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Find a track by file path.
    fn find_by_path(
        &self,
        path: &Path,
    ) -> impl Future<Output = Result<Option<Track>, StorageError>> + Send;

    /// Find tracks by content hash.
    fn find_by_hash(
        &self,
        hash: &str,
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Find tracks by metadata fingerprint.
    fn find_by_metadata_fingerprint(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Insert multiple tracks in a batch, returning their ids.
    fn insert_tracks_batch(
        &self,
        tracks: Vec<NewTrack>,
    ) -> impl Future<Output = Result<Vec<i64>, StorageError>> + Send;

    /// Find tracks by multiple file paths in a batch.
    fn find_by_paths_batch(
        &self,
        paths: &[&Path],
    ) -> impl Future<Output = Result<Vec<Option<Track>>, StorageError>> + Send;

    /// Find tracks by multiple content hashes in a batch.
    fn find_by_hashes_batch(
        &self,
        hashes: &[&str],
    ) -> impl Future<Output = Result<Vec<Vec<Track>>, StorageError>> + Send;

    /// Get tracks belonging to multiple albums in a single query.
    fn get_tracks_by_albums(
        &self,
        album_ids: &[i64],
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Get multiple tracks by their IDs in a single query.
    fn get_tracks_by_ids(
        &self,
        ids: &[i64],
    ) -> impl Future<Output = Result<Vec<Track>, StorageError>> + Send;

    /// Check if any track exists with the given content hash (exists-only query).
    fn hash_exists(&self, hash: &str) -> impl Future<Output = Result<bool, StorageError>> + Send;

    /// Delete orphan albums and artists (no remaining tracks/albums).
    fn prune_orphans(&self) -> impl Future<Output = Result<(), StorageError>> + Send;
}

/// Error type for storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Database error.
    #[error("Database error: {0}")]
    Database(String),
    /// Entity not found.
    #[error("Entity not found: {0}")]
    NotFound(String),
    /// Duplicate entry.
    #[error("Duplicate entry: {0}")]
    Duplicate(String),
    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),
    /// Invalid path.
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    /// Queue append rejected because the 100,000-entry cap was reached (FR-021).
    #[error("Queue full: maximum {max} entries")]
    QueueFull {
        /// Maximum number of queue entries allowed.
        max: usize,
    },
}

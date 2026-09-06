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

use std::{collections::HashMap, future::Future, path::Path, result::Result};

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
    fn insert_track(&self, track: NewTrack) -> impl Future<Output = StorageResult<i64>> + Send;

    /// Update an existing track.
    fn update_track(
        &self,
        id: i64,
        track: TrackUpdate,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Delete a track by id.
    fn delete_track(&self, id: i64) -> impl Future<Output = StorageResult<()>> + Send;

    /// Get a track by id.
    fn get_track(&self, id: i64) -> impl Future<Output = StorageResult<Option<Track>>> + Send;

    /// Get all tracks belonging to an album.
    fn get_tracks_by_album(
        &self,
        album_id: i64,
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Get all tracks by an artist.
    fn get_tracks_by_artist(
        &self,
        artist_id: i64,
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Search tracks by query string.
    fn search_tracks(&self, query: &str) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Insert a new album, returning its id.
    fn insert_album(&self, album: NewAlbum) -> impl Future<Output = StorageResult<i64>> + Send;

    /// Get an album by id.
    fn get_album(&self, id: i64) -> impl Future<Output = StorageResult<Option<Album>>> + Send;

    /// Get all albums.
    fn get_all_albums(&self) -> impl Future<Output = StorageResult<Vec<Album>>> + Send;

    /// Get distinct format info for a single album.
    fn get_album_format_info(
        &self,
        album_id: i64,
    ) -> impl Future<Output = StorageResult<FormatInfo>> + Send;

    /// Get distinct format info for multiple albums at once.
    fn get_albums_format_info(
        &self,
        album_ids: &[i64],
    ) -> impl Future<Output = StorageResult<HashMap<i64, FormatInfo>>> + Send;

    /// Get all albums by an artist.
    fn get_albums_by_artist(
        &self,
        artist_id: i64,
    ) -> impl Future<Output = StorageResult<Vec<Album>>> + Send;

    /// Insert a new artist, returning its id.
    fn insert_artist(&self, artist: NewArtist) -> impl Future<Output = StorageResult<i64>> + Send;

    /// Get an artist by id.
    fn get_artist(&self, id: i64) -> impl Future<Output = StorageResult<Option<Artist>>> + Send;

    /// Get all artists.
    fn get_all_artists(&self) -> impl Future<Output = StorageResult<Vec<Artist>>> + Send;

    /// List all configured library directories.
    fn list_library_directories(
        &self,
    ) -> impl Future<Output = StorageResult<Vec<LibraryDirectory>>> + Send;

    /// Add a library directory.
    fn add_library_directory(&self, path: &Path) -> impl Future<Output = StorageResult<()>> + Send;

    /// Remove a library directory by id.
    fn remove_library_directory(&self, id: i64) -> impl Future<Output = StorageResult<()>> + Send;

    /// Get the current playback queue.
    fn get_queue(&self) -> impl Future<Output = StorageResult<Vec<QueueEntry>>> + Send;

    /// Replace the entire queue.
    fn set_queue(
        &self,
        entries: &[NewQueueEntry],
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Append a track to the end of the queue.
    fn append_queue(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Remove a queue entry by id.
    fn remove_queue_entry(&self, id: i64) -> impl Future<Output = StorageResult<()>> + Send;

    /// Move a queue entry to a new position.
    fn reorder_queue(
        &self,
        entry_id: i64,
        new_position: u32,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Clear the entire queue.
    fn clear_queue(&self) -> impl Future<Output = StorageResult<()>> + Send;

    /// Find a track by file path.
    fn find_by_path(
        &self,
        path: &Path,
    ) -> impl Future<Output = StorageResult<Option<Track>>> + Send;

    /// Find tracks by content hash.
    fn find_by_hash(&self, hash: &str) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Find tracks by metadata fingerprint.
    fn find_by_metadata_fingerprint(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Insert multiple tracks in a batch, returning their ids.
    fn insert_tracks_batch(
        &self,
        tracks: Vec<NewTrack>,
    ) -> impl Future<Output = StorageResult<Vec<i64>>> + Send;

    /// Find tracks by multiple file paths in a batch.
    fn find_by_paths_batch(
        &self,
        paths: &[&Path],
    ) -> impl Future<Output = StorageResult<Vec<Option<Track>>>> + Send;

    /// Find tracks by multiple content hashes in a batch.
    fn find_by_hashes_batch(
        &self,
        hashes: &[&str],
    ) -> impl Future<Output = StorageResult<Vec<Vec<Track>>>> + Send;

    /// Get tracks belonging to multiple albums in a single query.
    fn get_tracks_by_albums(
        &self,
        album_ids: &[i64],
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Get multiple tracks by their IDs in a single query.
    fn get_tracks_by_ids(
        &self,
        ids: &[i64],
    ) -> impl Future<Output = StorageResult<Vec<Track>>> + Send;

    /// Check if any track exists with the given content hash (exists-only query).
    fn hash_exists(&self, hash: &str) -> impl Future<Output = StorageResult<bool>> + Send;

    /// Delete orphan albums and artists (no remaining tracks/albums).
    fn prune_orphans(&self) -> impl Future<Output = StorageResult<()>> + Send;
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
    QueueFull { max: usize },
}

/// Convenience alias for storage operation results.
pub type StorageResult<T> = Result<T, StorageError>;

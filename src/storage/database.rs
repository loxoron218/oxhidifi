//! `SQLite` database implementation using `sqlx` for library catalog persistence.
//!
//! The struct definition and the [`Storage`] trait impl live here; per-domain
//! query logic is implemented as `pub` inherent helpers in the sibling
//! `database/` modules. `pub` fields let those helpers read the pool
//! and settings without duplicating accessors.

pub mod albums;
pub mod artists;
pub mod disk_sync;
pub mod geometry_session;
pub mod music_dirs;
pub mod queue;
pub mod tracks;
pub mod user_prefs;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64},
    time::{Duration, Instant},
};

use {
    parking_lot::{Mutex, RwLock},
    sqlx::{
        SqlitePool,
        sqlite::{
            SqliteConnectOptions, SqliteJournalMode::Wal, SqlitePoolOptions,
            SqliteSynchronous::Normal,
        },
    },
};

use crate::{
    app::xdg_paths::dirs_config_home,
    storage::{
        Storage,
        StorageError::Database,
        StorageResult,
        catalog::{
            Album, Artist, LibraryDirectory, NewAlbum, NewArtist, NewQueueEntry, NewTrack,
            QueueContext, QueueEntry, Track, TrackUpdate,
        },
        config::persistence::SettingsStore,
        formats::FormatInfo,
        migrations::run,
    },
};

/// SQLite-backed storage implementation.
#[derive(Debug)]
pub struct SqliteStorage {
    /// `SQLite` connection pool.
    pub pool: SqlitePool,
    /// User settings store.
    pub settings: RwLock<SettingsStore>,
    /// Monotonically increasing sequence number for settings save requests.
    /// Used by the debounce logic to determine which request is the latest.
    pub last_save_seq: AtomicU64,
    /// Tracks the `(sequence, Instant)` of the most recent settings-save
    /// request so that concurrent callers can debounce correctly without
    /// losing the final write.
    pub last_save_req: Mutex<(u64, Instant)>,
    /// Set while a settings write is in flight, guaranteeing that at most
    /// one `spawn_blocking` disk write runs at a time.
    pub write_in_flight: AtomicBool,
    /// Set when a save request arrived while a write was in flight; the
    /// completing writer re-runs so the newest in-memory state always lands.
    pub write_pending: AtomicBool,
}

impl SqliteStorage {
    /// Create a new `SqliteStorage` with a connection pool to the given database path.
    ///
    /// Runs migrations on connect and loads settings from the default XDG
    /// config path.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created or migrations fail.
    pub async fn connect(database_path: &Path) -> StorageResult<Self> {
        let settings_path = dirs_config_home()
            .map_err(|e| Database(format!("Failed to resolve config dir: {e}")))?
            .join("oxhidifi")
            .join("settings.json");
        Self::connect_with_settings_path(database_path, &settings_path).await
    }

    /// Create a new `SqliteStorage` with a connection pool to the given database
    /// path and a custom settings file path.
    ///
    /// Runs migrations on connect. The settings path seam lets tests target a
    /// temporary directory instead of the user's real config.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created or migrations fail.
    pub async fn connect_with_settings_path(
        database_path: &Path,
        settings_path: &Path,
    ) -> StorageResult<Self> {
        let is_memory = database_path.as_os_str() == ":memory:";
        let mut opts = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true);
        if !is_memory {
            opts = opts.journal_mode(Wal).synchronous(Normal);
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(if is_memory { 1 } else { 8 })
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(opts)
            .await
            .map_err(|e| Database(format!("Failed to connect: {e}")))?;

        run(&pool).await?;

        let settings = SettingsStore::load_from_path(settings_path)
            .await
            .map_err(|e| Database(format!("Failed to load settings: {e}")))?;

        Ok(Self {
            pool,
            settings: RwLock::new(settings),
            last_save_seq: AtomicU64::new(0),
            last_save_req: Mutex::new((0, Instant::now())),
            write_in_flight: AtomicBool::new(false),
            write_pending: AtomicBool::new(false),
        })
    }

    /// Load file paths for a list of track IDs.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn get_track_paths(&self, ids: &[i64]) -> StorageResult<HashMap<i64, PathBuf>> {
        let tracks = self.get_tracks_by_ids(ids).await?;
        Ok(tracks
            .into_iter()
            .map(|t| (t.id, PathBuf::from(t.audio.file_path)))
            .collect())
    }
}

impl Storage for SqliteStorage {
    async fn insert_track(&self, track: NewTrack) -> StorageResult<i64> {
        self.insert_track_row(&track).await
    }

    async fn update_track(&self, id: i64, track: TrackUpdate) -> StorageResult<()> {
        self.update_track_row(id, track).await
    }

    async fn delete_track(&self, id: i64) -> StorageResult<()> {
        self.delete_track_row(id).await
    }

    async fn get_track(&self, id: i64) -> StorageResult<Option<Track>> {
        self.get_track_row(id).await
    }

    async fn get_tracks_by_album(&self, album_id: i64) -> StorageResult<Vec<Track>> {
        self.tracks_by_album(album_id).await
    }

    async fn get_tracks_by_artist(&self, artist_id: i64) -> StorageResult<Vec<Track>> {
        self.tracks_by_artist(artist_id).await
    }

    async fn search_tracks(&self, query: &str) -> StorageResult<Vec<Track>> {
        self.search_track_rows(query).await
    }

    async fn insert_album(&self, album: NewAlbum) -> StorageResult<i64> {
        self.insert_album_row(&album).await
    }

    async fn get_album(&self, id: i64) -> StorageResult<Option<Album>> {
        self.get_album_row(id).await
    }

    async fn get_all_albums(&self) -> StorageResult<Vec<Album>> {
        self.all_albums_rows().await
    }

    async fn get_album_format_info(&self, album_id: i64) -> StorageResult<FormatInfo> {
        self.album_format_info_rows(album_id).await
    }

    async fn get_albums_format_info(
        &self,
        album_ids: &[i64],
    ) -> StorageResult<HashMap<i64, FormatInfo>> {
        self.albums_format_info_rows(album_ids).await
    }

    async fn get_albums_by_artist(&self, artist_id: i64) -> StorageResult<Vec<Album>> {
        self.albums_by_artist_rows(artist_id).await
    }

    async fn insert_artist(&self, artist: NewArtist) -> StorageResult<i64> {
        self.insert_artist_row(&artist).await
    }

    async fn get_artist(&self, id: i64) -> StorageResult<Option<Artist>> {
        self.get_artist_row(id).await
    }

    async fn get_all_artists(&self) -> StorageResult<Vec<Artist>> {
        self.all_artists_rows().await
    }

    async fn list_library_directories(&self) -> StorageResult<Vec<LibraryDirectory>> {
        self.list_library_directory_rows().await
    }

    async fn add_library_directory(&self, path: &Path) -> StorageResult<()> {
        self.add_library_directory_row(path).await
    }

    async fn remove_library_directory(&self, id: i64) -> StorageResult<()> {
        self.remove_library_directory_row(id).await
    }

    async fn get_queue(&self) -> StorageResult<Vec<QueueEntry>> {
        self.get_queue_rows().await
    }

    async fn set_queue(&self, entries: &[NewQueueEntry]) -> StorageResult<()> {
        self.set_queue_rows(entries).await
    }

    async fn append_queue(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> StorageResult<()> {
        self.append_queue_row(track_id, context).await
    }

    async fn remove_queue_entry(&self, id: i64) -> StorageResult<()> {
        self.remove_queue_entry_row(id).await
    }

    async fn reorder_queue(&self, entry_id: i64, new_position: u32) -> StorageResult<()> {
        self.reorder_queue_row(entry_id, new_position).await
    }

    async fn clear_queue(&self) -> StorageResult<()> {
        self.clear_queue_rows().await
    }

    async fn find_by_path(&self, path: &Path) -> StorageResult<Option<Track>> {
        self.find_by_path_row(path).await
    }

    async fn find_by_hash(&self, hash: &str) -> StorageResult<Vec<Track>> {
        self.find_by_hash_rows(hash).await
    }

    async fn find_by_metadata_fingerprint(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> StorageResult<Vec<Track>> {
        self.find_by_fingerprint_rows(artist, album, title, track)
            .await
    }

    async fn insert_tracks_batch(&self, tracks: Vec<NewTrack>) -> StorageResult<Vec<i64>> {
        self.insert_tracks_batch_rows(tracks).await
    }

    async fn find_by_paths_batch(&self, paths: &[&Path]) -> StorageResult<Vec<Option<Track>>> {
        self.find_by_paths_batch_rows(paths).await
    }

    async fn find_by_hashes_batch(&self, hashes: &[&str]) -> StorageResult<Vec<Vec<Track>>> {
        self.find_by_hashes_batch_rows(hashes).await
    }

    async fn get_tracks_by_albums(&self, album_ids: &[i64]) -> StorageResult<Vec<Track>> {
        self.tracks_by_albums_rows(album_ids).await
    }

    async fn get_tracks_by_ids(&self, ids: &[i64]) -> StorageResult<Vec<Track>> {
        self.tracks_by_ids_rows(ids).await
    }

    async fn hash_exists(&self, hash: &str) -> StorageResult<bool> {
        self.hash_exists_row(hash).await
    }

    async fn prune_orphans(&self) -> StorageResult<()> {
        self.prune_orphans_row().await
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Context, Result},
        tempfile::TempDir,
    };

    use crate::storage::database::SqliteStorage;

    /// Connect to a temp database and settings file for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if the temporary storage backend fails to connect.
    pub(super) async fn storage_in(dir: &TempDir) -> Result<SqliteStorage> {
        let db = dir.path().join("library.db");
        let settings = dir.path().join("settings.json");
        SqliteStorage::connect_with_settings_path(&db, &settings)
            .await
            .context("storage should connect")
    }
}

//! `SQLite` database implementation using `sqlx` for library catalog persistence.
//!
//! The struct definition and the [`Storage`] trait impl live here; per-domain
//! query logic is implemented as `pub` inherent helpers in the sibling
//! `database/` modules. `pub` fields let those helpers read the pool
//! and settings without duplicating accessors.

pub mod albums;
pub mod artist_prefs;
pub mod artists;
pub mod disk_sync;
pub mod geometry_session;
pub mod music_dirs;
pub mod queue;
pub mod storage_impl;
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
        StorageError::{self, Database},
        config::persistence::SettingsStore,
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
    pub async fn connect(database_path: &Path) -> Result<Self, StorageError> {
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
    ) -> Result<Self, StorageError> {
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
    pub async fn get_track_paths(
        &self,
        ids: &[i64],
    ) -> Result<HashMap<i64, PathBuf>, StorageError> {
        let tracks = self.get_tracks_by_ids(ids).await?;
        Ok(tracks
            .into_iter()
            .map(|t| (t.id, PathBuf::from(t.audio.file_path)))
            .collect())
    }
}

/// Test helpers for database integration tests.
#[cfg(test)]
pub mod tests {
    use {
        anyhow::{Context, Result, ensure},
        tempfile::{TempDir, tempdir},
        tokio::test,
    };

    use crate::storage::database::SqliteStorage;

    /// Connect to a temp database and settings file for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if the temporary storage backend fails to connect.
    pub async fn storage_in(dir: &TempDir) -> Result<SqliteStorage> {
        let db = dir.path().join("library.db");
        let settings = dir.path().join("settings.json");
        SqliteStorage::connect_with_settings_path(&db, &settings)
            .await
            .context("storage should connect")
    }

    #[test]
    async fn storage_in_connects_to_temp_database() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;
        ensure!(
            storage.get_queue_rows().await?.is_empty(),
            "fresh temp database must start with an empty queue"
        );
        Ok(())
    }
}

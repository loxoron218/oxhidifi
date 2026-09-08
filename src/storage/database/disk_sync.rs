//! Debounced, serialized settings persistence to disk.

use std::{
    fs::write,
    sync::{Arc, atomic::Ordering::Relaxed},
    time::Instant,
};

use {
    serde_json::to_string_pretty,
    tokio::{
        spawn,
        task::spawn_blocking,
        time::{Duration, sleep},
    },
    tracing::warn,
};

use crate::storage::{
    StorageError::{self, Database},
    database::SqliteStorage,
};

impl SqliteStorage {
    /// Wait until 100 ms have elapsed since the most recent save request,
    /// or bail early if a newer request supersedes `my_seq`.
    /// Returns `true` when this caller should proceed with the write.
    pub async fn debounce_save(&self, my_seq: u64) -> bool {
        loop {
            let (latest_seq, latest_time) = *self.last_save_req.lock();
            let elapsed = latest_time.elapsed();
            match Duration::from_millis(100).checked_sub(elapsed) {
                Some(remaining) => sleep(remaining).await,
                None => return latest_seq == my_seq,
            }
        }
    }

    /// Debounce a settings save and write it to disk, draining any request
    /// that arrives while the write is in flight.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Database` if serialization or the file write fails.
    pub async fn save_settings_async(&self) -> Result<(), StorageError> {
        drain_settings_saves(self).await
    }

    /// Serialize the current in-memory settings and write them to disk.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Database` if serialization or the file write fails.
    pub async fn write_settings_to_disk(&self) -> Result<(), StorageError> {
        let json = to_string_pretty(self.settings.read().get())
            .map_err(|e| Database(format!("Failed to serialize settings: {e}")))?;
        let path = self.settings.read().path().to_path_buf();
        let path_for_error = path.clone();
        spawn_blocking(move || write(&path, &json))
            .await
            .map_err(|e| Database(e.to_string()))?
            .map_err(|e| {
                Database(format!(
                    "Failed to write settings {}: {e}",
                    path_for_error.display()
                ))
            })
    }

    /// Schedule a debounced settings write to disk in the background.
    ///
    /// In-memory settings are committed synchronously by the `*_memory`
    /// setters; this method coalesces rapid changes into a single disk
    /// write.  Errors are logged on the background task.
    pub fn save_settings(self: &Arc<Self>) {
        let me = Arc::clone(self);
        drop(spawn(async move {
            Self::persist_settings(&me).await;
        }));
    }

    /// Write settings to disk with a debounce, logging any failure.
    pub async fn persist_settings(storage: &Self) {
        if let Err(e) = storage.save_settings_async().await {
            warn!(error = %e, "Failed to persist settings");
        }
    }
}

/// Keep writing settings to disk until no further request is pending.
///
/// Debounces by waiting for a 100 ms quiet period after the most recent
/// request: if a newer request arrives during the wait, this pass bails and
/// lets the newer request drive the write, so the *last* change in a burst
/// always persists.
///
/// Writes are serialized through [`SqliteStorage::write_in_flight`]: at most
/// one disk write runs at a time. A request that arrives mid-write marks
/// [`SqliteStorage::write_pending`] and the completing pass drains it, so the
/// newest in-memory state always lands without concurrent writers racing.
/// Free function so the loop body stays below the clippy nesting threshold
/// (a method would add an extra level).
///
/// # Errors
///
/// Returns `StorageError::Database` if serialization or the file write fails.
async fn drain_settings_saves(storage: &SqliteStorage) -> Result<(), StorageError> {
    loop {
        let my_seq = storage.last_save_seq.fetch_add(1, Relaxed);
        *storage.last_save_req.lock() = (my_seq, Instant::now());

        if !storage.debounce_save(my_seq).await {
            return Ok(());
        }

        if storage.write_in_flight.swap(true, Relaxed) {
            storage.write_pending.store(true, Relaxed);
            return Ok(());
        }

        let result = storage.write_settings_to_disk().await;
        storage.write_in_flight.store(false, Relaxed);
        if storage.write_pending.swap(false, Relaxed) {
            continue;
        }
        return result;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{read_to_string, remove_dir_all, write},
        path::Path,
        sync::{Arc, atomic::Ordering::Relaxed},
    };

    use {
        anyhow::{Context, Result, bail, ensure},
        serde_json::from_str,
        tempfile::{TempDir, tempdir},
        tokio::{
            test,
            time::{Duration, sleep, timeout},
        },
    };

    use crate::storage::{
        StorageError::Database,
        database::{SqliteStorage, tests::storage_in},
        settings::UserSettings,
    };

    async fn wait_for_quiet_write(path: &Path, storage: &SqliteStorage) {
        while !path.exists()
            || storage.write_in_flight.load(Relaxed)
            || storage.write_pending.load(Relaxed)
        {
            sleep(Duration::from_millis(10)).await;
        }
    }

    async fn await_write_drain(storage: &SqliteStorage) -> Result<()> {
        let path = storage.settings.read().path().to_path_buf();
        timeout(Duration::from_secs(2), wait_for_quiet_write(&path, storage))
            .await
            .context("settings write should drain")
    }

    fn burst_zoom_saves(storage: &Arc<SqliteStorage>) {
        for level in 0..10 {
            storage.set_grid_zoom_level_memory(level);
            storage.save_settings();
        }
    }

    fn read_grid_zoom(dir: &TempDir) -> Result<u8> {
        let content = read_to_string(dir.path().join("settings.json"))?;
        let restored: UserSettings = from_str(&content)?;
        Ok(restored.grid_zoom_level)
    }

    #[test]
    async fn save_settings_persists_latest_value() -> Result<()> {
        let dir = tempdir()?;
        let storage = Arc::new(storage_in(&dir).await?);

        storage.set_grid_zoom_level_memory(3);
        storage.save_settings();

        await_write_drain(&storage).await?;
        ensure!(
            read_grid_zoom(&dir)? == 3,
            "latest in-memory value must persist"
        );
        Ok(())
    }

    #[test]
    async fn burst_saves_coalesce_to_latest_value() -> Result<()> {
        let dir = tempdir()?;
        let storage = Arc::new(storage_in(&dir).await?);

        burst_zoom_saves(&storage);

        await_write_drain(&storage).await?;
        ensure!(read_grid_zoom(&dir)? == 9, "newest burst value must win");
        Ok(())
    }

    #[test]
    async fn save_error_includes_settings_path() -> Result<()> {
        let dir = tempdir()?;
        let settings_dir = dir.path().join("settings_dir");
        let settings = settings_dir.join("settings.json");
        let db = dir.path().join("library.db");
        let storage = SqliteStorage::connect_with_settings_path(&db, &settings)
            .await
            .context("storage should connect")?;

        remove_dir_all(&settings_dir)?;
        write(&settings_dir, b"x")?;

        let message = match storage.save_settings_async().await {
            Err(Database(m)) => m,
            other => bail!("expected Database error, got {other:?}"),
        };
        ensure!(message.contains("settings.json"), "message was: {message}");
        ensure!(
            message.contains("Failed to write settings"),
            "message was: {message}"
        );
        Ok(())
    }
}

//! Window geometry and session persistence for `SqliteStorage`.
//!
//! Extracted from `user_prefs.rs` to keep each file under 400 lines.

use crate::storage::{
    StorageError::{self, Database},
    database::SqliteStorage,
};

impl SqliteStorage {
    /// Get the window width from settings.
    pub fn get_window_width(&self) -> i32 {
        self.settings.read().get().window.width
    }

    /// Get the window height from settings.
    pub fn get_window_height(&self) -> i32 {
        self.settings.read().get().window.height
    }

    /// Get the window maximized state from settings.
    pub fn get_window_maximized(&self) -> bool {
        self.settings.read().get().window.maximized
    }

    /// Get the window geometry (width, height, maximized).
    pub fn get_window_geometry(&self) -> (i32, i32, bool) {
        let s = self.settings.read();
        let cfg = s.get();
        let result = (cfg.window.width, cfg.window.height, cfg.window.maximized);
        drop(s);
        result
    }

    /// Get whether the side player panel (sidebar) is visible.
    pub fn get_sidebar_visible(&self) -> bool {
        self.settings.read().get_sidebar_visible()
    }

    /// Set the sidebar visibility in memory only.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_sidebar_visible_memory(&self, visible: bool) {
        self.settings
            .write()
            .update_memory(|s| s.window.sidebar_visible = visible);
    }

    /// Persist the sidebar visibility synchronously (used on close-request).
    ///
    /// # Errors
    ///
    /// Returns an error if the settings file cannot be written.
    pub fn set_sidebar_visible_sync(&self, visible: bool) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.window.sidebar_visible = visible);
        self.settings
            .read()
            .save_sync()
            .map_err(|e| Database(format!("Failed to save sidebar state: {e}")))
    }

    /// Persist window geometry synchronously (used on close-request).
    ///
    /// # Errors
    ///
    /// Returns an error if the settings file cannot be written.
    pub fn set_window_geometry_sync(
        &self,
        width: i32,
        height: i32,
        maximized: bool,
    ) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| {
            s.window.width = width;
            s.window.height = height;
            s.window.maximized = maximized;
        });
        self.settings
            .read()
            .save_sync()
            .map_err(|e| Database(format!("Failed to save window geometry: {e}")))
    }

    /// Get the last playback session data from settings.
    pub fn get_last_session(&self) -> (Vec<i64>, Option<usize>, Option<i64>, f64, f64) {
        self.settings.read().get_last_session()
    }

    /// Persist the current playback session to settings synchronously.
    ///
    /// Used on window close, where the write must complete before the
    /// process exits.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings file cannot be written.
    pub fn set_last_session(
        &self,
        queue: Vec<i64>,
        queue_index: Option<usize>,
        track_id: Option<i64>,
        position: f64,
        duration: f64,
    ) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| {
            s.last_queue = queue;
            s.last_queue_index = queue_index;
            s.last_track_id = track_id;
            s.last_position = position;
            s.last_duration = duration;
        });
        self.settings
            .read()
            .save_sync()
            .map_err(|e| Database(format!("Failed to save session: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        tempfile::tempdir,
        tokio::test,
    };

    use crate::storage::database::{SqliteStorage, tests::storage_in};

    #[test]
    async fn sidebar_visible_defaults_to_hidden() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;
        ensure!(
            !storage.get_sidebar_visible(),
            "sidebar must default to hidden for fresh settings"
        );
        Ok(())
    }

    #[test]
    async fn sidebar_visible_memory_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;
        storage.set_sidebar_visible_memory(true);
        ensure!(
            storage.get_sidebar_visible(),
            "sidebar must round-trip through the memory setter"
        );
        storage.set_sidebar_visible_memory(false);
        ensure!(
            !storage.get_sidebar_visible(),
            "sidebar must round-trip back to hidden"
        );
        Ok(())
    }

    #[test]
    async fn sidebar_visible_sync_persists_to_disk() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("library.db");
        let settings_path = dir.path().join("settings.json");
        let storage = SqliteStorage::connect_with_settings_path(&db_path, &settings_path).await?;
        storage.set_sidebar_visible_sync(true)?;
        ensure!(
            settings_path.exists(),
            "settings file must be written by the sync setter"
        );
        drop(storage);
        let reloaded = SqliteStorage::connect_with_settings_path(&db_path, &settings_path).await?;
        ensure!(
            reloaded.get_sidebar_visible(),
            "sidebar visibility must survive a storage reload"
        );
        Ok(())
    }
}

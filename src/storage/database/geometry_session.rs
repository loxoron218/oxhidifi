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
        self.settings.read().get().window_width
    }

    /// Get the window height from settings.
    pub fn get_window_height(&self) -> i32 {
        self.settings.read().get().window_height
    }

    /// Get the window maximized state from settings.
    pub fn get_window_maximized(&self) -> bool {
        self.settings.read().get().window_maximized
    }

    /// Get the window geometry (width, height, maximized).
    pub fn get_window_geometry(&self) -> (i32, i32, bool) {
        let s = self.settings.read();
        let cfg = s.get();
        let result = (cfg.window_width, cfg.window_height, cfg.window_maximized);
        drop(s);
        result
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
            s.window_width = width;
            s.window_height = height;
            s.window_maximized = maximized;
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

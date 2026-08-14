//! In-memory user preference API on `SqliteStorage`.

use crate::{
    playback::devices::OutputMode,
    storage::{
        StorageError::{self, Database},
        active_tab::ActiveTab,
        database::SqliteStorage,
        sort_rules::{AlbumSortItem, ArtistSortItem},
        view_mode::ViewMode,
    },
};

impl SqliteStorage {
    /// Get the current view mode.
    pub fn get_view_mode(&self) -> ViewMode {
        self.settings.read().get().view_mode
    }

    /// Set the view mode in memory and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings cannot be saved.
    pub async fn set_view_mode(&self, mode: ViewMode) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| s.view_mode = mode);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save view mode: {e}")))?;
        Ok(())
    }

    /// Get whether gapless playback is enabled.
    pub fn get_gapless_enabled(&self) -> bool {
        self.settings.read().get_gapless_enabled()
    }

    /// Set whether gapless playback is enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_gapless_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.gapless_enabled = enabled);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save gapless setting: {e}")))?;
        Ok(())
    }

    /// Get whether album labels are shown under cover art.
    pub fn get_show_album_labels(&self) -> bool {
        self.settings.read().get_show_album_labels()
    }

    /// Set whether album labels are shown under cover art.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_show_album_labels(&self, enabled: bool) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.show_album_labels = enabled);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save album labels setting: {e}")))?;
        Ok(())
    }

    /// Get the preferred audio device name.
    pub fn get_audio_device(&self) -> Option<String> {
        self.settings.read().get_audio_device().map(String::from)
    }

    /// Set the preferred audio device name.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_audio_device(&self, device: Option<String>) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.audio_device = device);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save audio device: {e}")))?;
        Ok(())
    }

    /// Get the active tab preference.
    pub fn get_active_tab(&self) -> ActiveTab {
        self.settings.read().get_active_tab()
    }

    /// Set the active tab preference.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_active_tab(&self, tab: ActiveTab) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| s.active_tab = tab);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save active tab: {e}")))?;
        Ok(())
    }

    /// Get the albums sort configuration.
    pub fn get_albums_sort(&self) -> Vec<AlbumSortItem> {
        self.settings.read().get().albums_sort.clone()
    }

    /// Set the albums sort configuration in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_albums_sort_memory(&self, items: Vec<AlbumSortItem>) {
        self.settings
            .write()
            .update_memory(|s| s.albums_sort = items);
    }

    /// Get the artists sort configuration.
    pub fn get_artists_sort(&self) -> Vec<ArtistSortItem> {
        self.settings.read().get().artists_sort.clone()
    }

    /// Set the artists sort configuration in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_artists_sort_memory(&self, items: Vec<ArtistSortItem>) {
        self.settings
            .write()
            .update_memory(|s| s.artists_sort = items);
    }

    /// Get the grid zoom level.
    pub fn get_grid_zoom_level(&self) -> u8 {
        self.settings.read().get().grid_zoom_level
    }

    /// Set the grid zoom level in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_grid_zoom_level_memory(&self, level: u8) {
        self.settings
            .write()
            .update_memory(|s| s.grid_zoom_level = level);
    }

    /// Get the list zoom level.
    pub fn get_list_zoom_level(&self) -> u8 {
        self.settings.read().get().list_zoom_level
    }

    /// Set the list zoom level in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_list_zoom_level_memory(&self, level: u8) {
        self.settings
            .write()
            .update_memory(|s| s.list_zoom_level = level);
    }

    /// Get the volume level from settings.
    pub fn get_settings_volume(&self) -> f64 {
        self.settings.read().get_volume()
    }

    /// Set the volume level in memory and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings cannot be saved.
    pub async fn set_volume(&self, volume: f64) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| s.volume = volume);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save volume: {e}")))?;
        Ok(())
    }

    /// Set the volume level in memory only.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_volume_memory(&self, volume: f64) {
        self.settings.write().update_memory(|s| s.volume = volume);
    }

    /// Get the output mode from settings.
    pub fn get_output_mode(&self) -> OutputMode {
        self.settings.read().get_output_mode()
    }

    /// Set the output mode in memory and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings cannot be saved.
    pub async fn set_output_mode(&self, mode: OutputMode) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.output_mode = mode);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save output mode: {e}")))?;
        Ok(())
    }

    /// Set the output mode in memory only.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_output_mode_memory(&self, mode: OutputMode) {
        self.settings
            .write()
            .update_memory(|s| s.output_mode = mode);
    }

    /// Get the last playback session data from settings.
    pub fn get_last_session(&self) -> (Vec<i64>, Option<usize>, Option<i64>, f64, f64) {
        self.settings.read().get_last_session()
    }

    /// Persist the current playback session to settings synchronously.
    ///
    /// Used on window close, where the write must complete before the
    /// process exits. Bypasses the debounced async save, which may be
    /// dropped once the main loop stops before the debounce window
    /// elapses.
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

    use crate::{
        playback::devices::OutputMode::BitPerfect,
        storage::{
            active_tab::ActiveTab::Artists,
            database::tests::storage_in,
            sort_rules::{
                AlbumSortCriteria::{BitDepth, Title},
                AlbumSortItem,
                ArtistSortCriteria::Name,
                ArtistSortItem,
                SortOrder::{Ascending, Descending},
            },
        },
    };

    #[test]
    async fn albums_sort_memory_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        let items = vec![
            AlbumSortItem {
                criteria: Title,
                order: Descending,
            },
            AlbumSortItem {
                criteria: BitDepth,
                order: Ascending,
            },
        ];
        storage.set_albums_sort_memory(items.clone());
        ensure!(
            storage.get_albums_sort() == items,
            "albums sort must round-trip through the memory setters"
        );
        Ok(())
    }

    #[test]
    async fn artists_sort_memory_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        let items = vec![ArtistSortItem {
            criteria: Name,
            order: Descending,
        }];
        storage.set_artists_sort_memory(items.clone());
        ensure!(
            storage.get_artists_sort() == items,
            "artists sort must round-trip through the memory setters"
        );
        Ok(())
    }

    #[test]
    async fn zoom_levels_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_grid_zoom_level_memory(3);
        storage.set_list_zoom_level_memory(2);
        ensure!(
            storage.get_grid_zoom_level() == 3,
            "grid zoom must round-trip through the memory setter"
        );
        ensure!(
            storage.get_list_zoom_level() == 2,
            "list zoom must round-trip through the memory setter"
        );
        Ok(())
    }

    #[test]
    async fn volume_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_volume_memory(0.42);
        ensure!(
            (storage.get_settings_volume() - 0.42).abs() < f64::EPSILON,
            "volume must round-trip through the memory setter"
        );
        Ok(())
    }

    #[test]
    async fn output_mode_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_output_mode_memory(BitPerfect);
        ensure!(
            storage.get_output_mode() == BitPerfect,
            "output mode must round-trip through the memory setter"
        );
        Ok(())
    }

    #[test]
    async fn active_tab_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_active_tab(Artists).await?;
        ensure!(
            storage.get_active_tab() == Artists,
            "active tab must round-trip through the async setter"
        );
        Ok(())
    }
}

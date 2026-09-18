//! JSON settings persistence with corruption-recovery fallback.

use std::{
    fs::write,
    path::{Path, PathBuf},
};

use serde_json::to_string_pretty;

use crate::{
    app::xdg_paths::dirs_config_home,
    playback::devices::OutputMode,
    storage::{
        StorageError::{self, Database, Serialization},
        active_tab::ActiveTab,
        config::corrupt_recovery::{ensure_parent_dir, load_settings_with_fallback},
        settings::UserSettings,
    },
};

/// Manages persistent user settings stored as JSON.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    /// Path to the settings JSON file.
    pub settings_path: PathBuf,
    /// In-memory settings state.
    pub settings: UserSettings,
}

impl SettingsStore {
    /// Load settings from the XDG config path, creating defaults if missing.
    ///
    /// A malformed or unreadable settings file falls back to defaults rather
    /// than failing the whole load.
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory cannot be created.
    pub async fn load_async() -> Result<Self, StorageError> {
        let settings_path = dirs_config_home()
            .map_err(|e| Database(format!("Failed to resolve config dir: {e}")))?
            .join("oxhidifi")
            .join("settings.json");
        Self::load_from_path(&settings_path).await
    }

    /// Load settings from an explicit settings file path, creating the parent
    /// directory and falling back to defaults for a missing or malformed file.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings parent directory cannot be created.
    pub async fn load_from_path(settings_path: &Path) -> Result<Self, StorageError> {
        if let Some(dir) = settings_path.parent() {
            ensure_parent_dir(dir).await?;
        }

        let settings = load_settings_with_fallback(settings_path).await?;

        Ok(Self {
            settings_path: settings_path.to_path_buf(),
            settings,
        })
    }

    /// Synchronously update in-memory state only (no I/O).
    pub fn update_memory(&mut self, f: impl FnOnce(&mut UserSettings)) {
        f(&mut self.settings);
    }

    /// Serialize current settings and write to disk synchronously.
    ///
    /// Bypasses the async write path so the write is guaranteed to complete
    /// before the caller returns (e.g. on window close, when the process
    /// exits before a debounced async save would finish).
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or the file write fails.
    pub fn save_sync(&self) -> Result<(), StorageError> {
        let json = to_string_pretty(&self.settings)
            .map_err(|e| Serialization(format!("Failed to serialize settings: {e}")))?;
        write(&self.settings_path, &json).map_err(|e| {
            Database(format!(
                "Failed to write settings: {}: {e}",
                self.settings_path.display()
            ))
        })?;
        Ok(())
    }

    /// Get a reference to the current settings.
    #[must_use]
    pub const fn get(&self) -> &UserSettings {
        &self.settings
    }

    /// Get whether gapless playback is enabled.
    #[must_use]
    pub const fn get_gapless_enabled(&self) -> bool {
        self.settings.gapless_enabled
    }

    /// Get whether album labels are shown under cover art.
    #[must_use]
    pub const fn get_show_album_labels(&self) -> bool {
        self.settings.show_album_labels
    }

    /// Get the preferred audio device name.
    #[must_use]
    pub fn get_audio_device(&self) -> Option<&str> {
        self.settings.audio_device.as_deref()
    }

    /// Get the active tab preference.
    #[must_use]
    pub const fn get_active_tab(&self) -> ActiveTab {
        self.settings.active_tab
    }

    /// Get the volume level.
    #[must_use]
    pub const fn get_volume(&self) -> f64 {
        self.settings.volume
    }

    /// Get the output mode.
    #[must_use]
    pub const fn get_output_mode(&self) -> OutputMode {
        self.settings.output_mode
    }

    /// Get the last playback session data.
    #[must_use]
    pub fn get_last_session(&self) -> (Vec<i64>, Option<usize>, Option<i64>, f64, f64) {
        (
            self.settings.last_queue.clone(),
            self.settings.last_queue_index,
            self.settings.last_track_id,
            self.settings.last_position,
            self.settings.last_duration,
        )
    }

    /// Get read access to the underlying settings path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.settings_path
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{read_to_string, write},
        path::Path,
    };

    use {
        anyhow::{Result, bail, ensure},
        serde_json::{from_str, to_string_pretty},
        tempfile::{TempDir, tempdir},
        tokio::test as tokio_test,
    };

    use crate::storage::{config::persistence::SettingsStore, settings::UserSettings};

    fn store_in(dir: &Path) -> SettingsStore {
        SettingsStore {
            settings_path: dir.join("settings.json"),
            settings: UserSettings {
                volume: 0.5,
                ..UserSettings::default()
            },
        }
    }

    fn read_volume(dir: &TempDir) -> Result<f64> {
        let content = read_to_string(dir.path().join("settings.json"))?;
        let restored: UserSettings = from_str(&content)?;
        Ok(restored.volume)
    }

    #[test]
    fn show_album_labels_defaults_and_round_trips() {
        let Ok(dir) = tempdir() else { return };
        let mut store = SettingsStore {
            settings_path: dir.path().join("settings.json"),
            settings: UserSettings::default(),
        };
        assert!(
            store.get_show_album_labels(),
            "album labels should default to visible"
        );
        store.update_memory(|s| s.show_album_labels = false);
        assert!(
            !store.get_show_album_labels(),
            "album labels should reflect update_memory"
        );
        store.update_memory(|s| s.show_album_labels = true);
        assert!(
            store.get_show_album_labels(),
            "album labels should be re-enabled"
        );
    }

    #[test]
    fn last_session_round_trips() {
        let Ok(dir) = tempdir() else { return };
        let mut store = SettingsStore {
            settings_path: dir.path().join("settings.json"),
            settings: UserSettings::default(),
        };

        let (queue, index, track, position, duration) = store.get_last_session();
        assert!(queue.is_empty(), "default session queue should be empty");
        assert_eq!(index, None);
        assert_eq!(track, None);
        assert!((position - 0.0).abs() < f64::EPSILON);
        assert!((duration - 0.0).abs() < f64::EPSILON);

        store.update_memory(|s| {
            s.last_queue = vec![10, 20, 30];
            s.last_queue_index = Some(1);
            s.last_track_id = Some(20);
            s.last_position = 42.5;
            s.last_duration = 200.0;
        });

        let (queue, index, track, position, duration) = store.get_last_session();
        assert_eq!(queue, vec![10, 20, 30]);
        assert_eq!(index, Some(1));
        assert_eq!(track, Some(20));
        assert!((position - 42.5).abs() < f64::EPSILON);
        assert!((duration - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn save_sync_persists_file() -> Result<()> {
        let dir = tempdir()?;
        let store = store_in(dir.path());
        store.save_sync()?;
        ensure!((read_volume(&dir)? - 0.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn save_sync_error_includes_settings_path() -> Result<()> {
        let dir = tempdir()?;
        let store = SettingsStore {
            settings_path: dir.path().join("missing").join("settings.json"),
            settings: UserSettings::default(),
        };
        let message = match store.save_sync() {
            Ok(()) => bail!("expected save to fail for a missing directory"),
            Err(e) => e.to_string(),
        };
        ensure!(message.contains("settings.json"), "message was: {message}");
        ensure!(
            message.contains("Failed to write settings"),
            "message was: {message}"
        );
        Ok(())
    }

    #[tokio_test]
    async fn load_from_path_creates_parent_dir_and_returns_defaults() -> Result<()> {
        let dir = tempdir()?;
        let settings_path = dir.path().join("nested").join("settings.json");
        let store = SettingsStore::load_from_path(&settings_path).await?;
        ensure!(
            dir.path().join("nested").is_dir(),
            "load_from_path must create the settings parent directory"
        );
        ensure!(store.path() == settings_path);
        ensure!(
            (store.get_volume() - 1.0).abs() < f64::EPSILON,
            "a missing file must yield default settings"
        );
        Ok(())
    }

    #[tokio_test]
    async fn load_from_path_round_trips_valid_file() -> Result<()> {
        let dir = tempdir()?;
        let settings_path = dir.path().join("settings.json");
        let original = UserSettings {
            volume: 0.75,
            ..UserSettings::default()
        };
        write(&settings_path, to_string_pretty(&original)?)?;

        let store = SettingsStore::load_from_path(&settings_path).await?;
        ensure!(
            (store.get_volume() - 0.75).abs() < f64::EPSILON,
            "loaded volume must match the file contents"
        );
        Ok(())
    }

    #[tokio_test]
    async fn load_from_path_falls_back_to_defaults_on_corrupt_file() -> Result<()> {
        let dir = tempdir()?;
        let settings_path = dir.path().join("settings.json");
        write(&settings_path, "{ this is not valid json")?;

        let store = SettingsStore::load_from_path(&settings_path).await?;
        ensure!(
            (store.get_volume() - 1.0).abs() < f64::EPSILON,
            "a corrupt file must fall back to defaults instead of failing the load"
        );
        Ok(())
    }

    #[tokio_test]
    async fn load_from_path_backs_up_corrupt_file_before_defaults() -> Result<()> {
        let dir = tempdir()?;
        let settings_path = dir.path().join("settings.json");
        let corrupt = "{ this is not valid json";
        write(&settings_path, corrupt)?;

        let corrupt_store = SettingsStore::load_from_path(&settings_path).await?;
        ensure!(
            corrupt_store.settings_path == settings_path,
            "loaded store must reference the settings path"
        );

        ensure!(
            !settings_path.exists(),
            "the corrupt file must be renamed out of the way"
        );
        let backup_path = settings_path.with_extension("json.corrupt");
        ensure!(
            backup_path.exists(),
            "the corrupt file must be preserved as a backup"
        );
        ensure!(
            read_to_string(&backup_path)? == corrupt,
            "the backup must retain the original corrupt contents"
        );
        Ok(())
    }
}

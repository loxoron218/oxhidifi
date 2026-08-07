//! XDG-based user settings persistence using `serde_json`.

use std::{
    fs::write,
    path::{Path, PathBuf},
};

use {
    anyhow::{Context, Result},
    serde::{Deserialize, Serialize},
    serde_json::{from_str, to_string_pretty},
    tokio::fs::{create_dir_all, read_to_string, rename, try_exists},
    tracing::warn,
};

use crate::{
    app::dirs_config_home, playback::devices::OutputMode, storage::user_settings::UserSettings,
};

/// Active tab in the library view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ActiveTab {
    /// Albums tab.
    Albums,
    /// Artists tab.
    Artists,
}

/// Manages persistent user settings stored as JSON.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    /// Path to the settings JSON file.
    settings_path: PathBuf,
    /// In-memory settings state.
    settings: UserSettings,
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
    pub async fn load_async() -> Result<Self> {
        let settings_path = dirs_config_home()?.join("oxhidifi").join("settings.json");
        Self::load_from_path(&settings_path).await
    }

    /// Load settings from an explicit settings file path, creating the parent
    /// directory and falling back to defaults for a missing or malformed file.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings parent directory cannot be created.
    pub async fn load_from_path(settings_path: &Path) -> Result<Self> {
        if let Some(config_dir) = settings_path.parent() {
            create_dir_all(config_dir).await.context(format!(
                "Failed to create config directory: {}",
                config_dir.display()
            ))?;
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
    pub fn save_sync(&self) -> Result<()> {
        let json = to_string_pretty(&self.settings).context("Failed to serialize settings")?;
        write(&self.settings_path, &json).with_context(|| {
            format!("Failed to write settings: {}", self.settings_path.display())
        })?;
        Ok(())
    }

    /// Get a reference to the current settings.
    #[must_use]
    pub fn get(&self) -> &UserSettings {
        &self.settings
    }

    /// Get whether gapless playback is enabled.
    #[must_use]
    pub fn get_gapless_enabled(&self) -> bool {
        self.settings.gapless_enabled
    }

    /// Get whether album labels are shown under cover art.
    #[must_use]
    pub fn get_show_album_labels(&self) -> bool {
        self.settings.show_album_labels
    }

    /// Get the preferred audio device name.
    #[must_use]
    pub fn get_audio_device(&self) -> Option<&str> {
        self.settings.audio_device.as_deref()
    }

    /// Get the active tab preference.
    #[must_use]
    pub fn get_active_tab(&self) -> ActiveTab {
        self.settings.active_tab
    }

    /// Get the volume level.
    #[must_use]
    pub fn get_volume(&self) -> f64 {
        self.settings.volume
    }

    /// Get the output mode.
    #[must_use]
    pub fn get_output_mode(&self) -> OutputMode {
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

/// User-facing view mode preference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Grid layout.
    Grid,
    /// Column/list layout.
    Column,
}

impl ViewMode {
    /// Get the icon name for this view mode.
    #[must_use]
    pub const fn icon_name(self) -> &'static str {
        match self {
            Self::Grid => "view-grid-symbolic",
            Self::Column => "view-list-symbolic",
        }
    }

    /// Get the tooltip text for this view mode.
    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Grid => "Switch to column view",
            Self::Column => "Switch to grid view",
        }
    }
}

/// Try to load settings from file, falling back to defaults on parse error.
async fn load_settings_with_fallback(settings_path: &Path) -> Result<UserSettings> {
    if try_exists(settings_path).await.unwrap_or(false) {
        let content = read_to_string(settings_path)
            .await
            .with_context(|| format!("Failed to read settings: {}", settings_path.display()))?;
        match from_str(&content) {
            Ok(settings) => Ok(settings),
            Err(e) => {
                warn!(
                    error = %e,
                    path = %settings_path.display(),
                    "Failed to parse settings, falling back to defaults",
                );
                backup_corrupt_settings(settings_path).await;
                Ok(UserSettings::default())
            }
        }
    } else {
        Ok(UserSettings::default())
    }
}

/// Preserve a corrupt settings file before defaults overwrite it.
///
/// The fallback above returns defaults, which the next save writes back over
/// the corrupt file — destroying any recoverable data. Renaming the file out
/// of the way first keeps it for manual recovery. Best‑effort: a failed
/// rename only logs, never fails the load.
async fn backup_corrupt_settings(settings_path: &Path) {
    let backup_path = settings_path.with_extension("json.corrupt");
    if let Err(e) = rename(settings_path, &backup_path).await {
        warn!(
            error = %e,
            path = %backup_path.display(),
            "Failed to back up corrupt settings file",
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{File, read_to_string, write},
        io::BufReader,
        path::Path,
    };

    use {
        anyhow::{Result, bail, ensure},
        serde_json::{from_reader, from_str, to_string_pretty},
        tempfile::{TempDir, tempdir},
        tokio::test as tokio_test,
    };

    use crate::{
        playback::devices::OutputMode::{BitPerfect, Resampled},
        storage::{
            settings::{
                ActiveTab::{Albums, Artists},
                SettingsStore,
                ViewMode::{Column, Grid},
            },
            user_settings::UserSettings,
        },
    };

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
    fn settings_defaults() {
        let settings = UserSettings::default();
        assert!((settings.volume - 1.0).abs() < f64::EPSILON);
        assert_eq!(settings.view_mode, Grid);
        assert_eq!(settings.active_tab, Albums);
        assert_eq!(settings.window_width, 1200);
        assert!(!settings.window_maximized);
        assert_eq!(settings.output_mode, Resampled);
    }

    #[test]
    fn settings_round_trip() {
        let Ok(dir) = tempdir() else { return };
        let settings_path = dir.path().join("settings.json");

        let original = UserSettings {
            volume: 0.5,
            view_mode: Column,
            ..UserSettings::default()
        };

        let Ok(json) = to_string_pretty(&original) else {
            return;
        };
        let Ok(()) = write(&settings_path, &json) else {
            return;
        };

        let Ok(file) = File::open(&settings_path) else {
            return;
        };
        let reader = BufReader::new(file);
        let Ok(restored) = from_reader::<_, UserSettings>(reader) else {
            return;
        };

        assert!((restored.volume - 0.5).abs() < f64::EPSILON);
        assert_eq!(restored.view_mode, Column);
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
    fn active_tab_round_trips() {
        let Ok(dir) = tempdir() else { return };
        let mut store = SettingsStore {
            settings_path: dir.path().join("settings.json"),
            settings: UserSettings::default(),
        };
        assert_eq!(store.get_active_tab(), Albums);
        store.update_memory(|s| s.active_tab = Artists);
        assert_eq!(store.get_active_tab(), Artists);
    }

    #[test]
    fn output_mode_round_trips_through_user_settings() {
        let original = UserSettings {
            output_mode: BitPerfect,
            ..UserSettings::default()
        };
        let Ok(json) = to_string_pretty(&original) else {
            return;
        };
        assert!(
            json.contains("\"bit_perfect\""),
            "output_mode should serialize with snake_case tag"
        );
        let Ok(restored) = from_str::<UserSettings>(&json) else {
            return;
        };
        assert_eq!(restored.output_mode, BitPerfect);
        assert_eq!(restored.output_mode, original.output_mode);
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

        SettingsStore::load_from_path(&settings_path).await?;

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

//! XDG-based user settings persistence using `serde_json`.

use std::path::{Path, PathBuf};

use {
    anyhow::{Context, Result},
    serde::{Deserialize, Serialize},
    serde_json::{from_str, to_string_pretty},
    tokio::{
        fs::{create_dir_all, read_to_string, try_exists},
        task::spawn_blocking,
    },
};

use crate::app::dirs_config_home;

/// Active tab in the library view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ActiveTab {
    /// Albums tab.
    Albums,
    /// Artists tab.
    Artists,
}

/// Manages persistent user settings stored as JSON.
#[derive(Debug)]
pub struct SettingsStore {
    /// Path to the settings JSON file.
    settings_path: PathBuf,
    /// In-memory settings state.
    settings: UserSettings,
}

impl SettingsStore {
    /// Load settings from the XDG config path, creating defaults if missing.
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory cannot be created or the file
    /// cannot be read.
    pub async fn load_async() -> Result<Self> {
        let config_dir = dirs_config_home()?.join("oxhidifi");
        create_dir_all(&config_dir).await.with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        let settings_path = config_dir.join("settings.json");
        let settings = if try_exists(&settings_path).await.unwrap_or(false) {
            let content = read_to_string(&settings_path)
                .await
                .with_context(|| format!("Failed to read settings: {}", settings_path.display()))?;
            from_str(&content)
                .with_context(|| format!("Failed to parse settings: {}", settings_path.display()))?
        } else {
            UserSettings::default()
        };

        Ok(Self {
            settings_path,
            settings,
        })
    }

    /// Synchronously update in-memory state only (no I/O).
    pub fn update_memory(&mut self, f: impl FnOnce(&mut UserSettings)) {
        f(&mut self.settings);
    }

    /// Serialize current settings and persist to disk via `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or the file write fails.
    pub async fn save_async(&self) -> Result<()> {
        let json = to_string_pretty(&self.settings).context("Failed to serialize settings")?;
        let path = self.settings_path.clone();
        let path_for_error = path.clone();
        let join_result = spawn_blocking(move || std::fs::write(&path, &json)).await;
        join_result
            .context("Failed to spawn blocking write")?
            .with_context(|| format!("Failed to write settings: {}", path_for_error.display()))?;
        Ok(())
    }

    /// Get a reference to the current settings.
    #[must_use]
    pub fn get(&self) -> &UserSettings {
        &self.settings
    }

    /// Get a mutable reference to the current settings.
    pub fn get_mut(&mut self) -> &mut UserSettings {
        &mut self.settings
    }

    /// Update in-memory settings and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub async fn update_async(&mut self, f: impl FnOnce(&mut UserSettings)) -> Result<()> {
        self.update_memory(f);
        self.save_async().await
    }

    /// Get whether gapless playback is enabled.
    #[must_use]
    pub fn get_gapless_enabled(&self) -> bool {
        self.settings.gapless_enabled
    }

    /// Set whether gapless playback is enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub async fn set_gapless_enabled_async(&mut self, enabled: bool) -> Result<()> {
        self.update_async(|s| s.gapless_enabled = enabled).await
    }

    /// Get the preferred audio device name.
    #[must_use]
    pub fn get_audio_device(&self) -> Option<&str> {
        self.settings.audio_device.as_deref()
    }

    /// Set the preferred audio device name.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub async fn set_audio_device_async(&mut self, device: Option<String>) -> Result<()> {
        self.update_async(|s| s.audio_device = device).await
    }

    /// Get the active tab preference.
    #[must_use]
    pub fn get_active_tab(&self) -> ActiveTab {
        self.settings.active_tab
    }

    /// Set the active tab preference.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub async fn set_active_tab_async(&mut self, tab: ActiveTab) -> Result<()> {
        self.update_async(|s| s.active_tab = tab).await
    }

    /// Get the volume level.
    #[must_use]
    pub fn get_volume(&self) -> f64 {
        self.settings.volume
    }

    /// Set the volume level.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub async fn set_volume_async(&mut self, volume: f64) -> Result<()> {
        self.update_async(|s| s.volume = volume).await
    }

    /// Get read access to the underlying settings path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.settings_path
    }
}

/// Persistent user settings stored as JSON at XDG config path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    /// Configured library directory paths.
    pub library_directories: Vec<String>,
    /// Preferred audio output device name (None = default).
    pub audio_device: Option<String>,
    /// Playback volume (0.0–1.0).
    pub volume: f64,
    /// Current view mode preference.
    pub view_mode: ViewMode,
    /// Last active tab.
    pub active_tab: ActiveTab,
    /// Stored window width.
    pub window_width: i32,
    /// Stored window height.
    pub window_height: i32,
    /// Whether window is maximized.
    pub window_maximized: bool,
    /// Whether gapless playback is enabled.
    pub gapless_enabled: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            library_directories: Vec::new(),
            audio_device: None,
            volume: 0.8,
            view_mode: ViewMode::Grid,
            active_tab: ActiveTab::Albums,
            window_width: 1200,
            window_height: 800,
            window_maximized: false,
            gapless_enabled: true,
        }
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

#[cfg(test)]
mod tests {
    use std::{
        fs::{File, write},
        io::BufReader,
    };

    use {
        serde_json::{from_reader, to_string_pretty},
        tempfile::tempdir,
    };

    use crate::storage::settings::{
        ActiveTab::Albums,
        UserSettings,
        ViewMode::{Column, Grid},
    };

    #[test]
    fn settings_defaults() {
        let settings = UserSettings::default();
        assert!(settings.library_directories.is_empty());
        assert!((settings.volume - 0.8).abs() < f64::EPSILON);
        assert_eq!(settings.view_mode, Grid);
        assert_eq!(settings.active_tab, Albums);
        assert_eq!(settings.window_width, 1200);
        assert!(!settings.window_maximized);
    }

    #[test]
    fn settings_round_trip() {
        let Ok(dir) = tempdir() else { return };
        let settings_path = dir.path().join("settings.json");

        let original = UserSettings {
            library_directories: vec!["/music".to_string()],
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

        assert_eq!(restored.library_directories, original.library_directories);
        assert!((restored.volume - 0.5).abs() < f64::EPSILON);
        assert_eq!(restored.view_mode, Column);
    }
}

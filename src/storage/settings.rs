//! XDG-based user settings persistence using `serde_json`.

use std::{
    fs::{File, create_dir_all, write},
    io::BufReader,
    path::PathBuf,
};

use {
    anyhow::{Context, Result},
    serde::{Deserialize, Serialize},
    serde_json::{from_reader, to_string_pretty},
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
    pub fn load() -> Result<Self> {
        let config_dir = dirs_config_home()?.join("oxhidifi");
        create_dir_all(&config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        let settings_path = config_dir.join("settings.json");
        let settings = if settings_path.exists() {
            let file = File::open(&settings_path)
                .with_context(|| format!("Failed to open settings: {}", settings_path.display()))?;
            let reader = BufReader::new(file);
            from_reader(reader)
                .with_context(|| format!("Failed to parse settings: {}", settings_path.display()))?
        } else {
            UserSettings::default()
        };

        Ok(Self {
            settings_path,
            settings,
        })
    }

    /// Save current settings to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self) -> Result<()> {
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

    /// Get a mutable reference to the current settings.
    pub fn get_mut(&mut self) -> &mut UserSettings {
        &mut self.settings
    }

    /// Update settings and persist to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn update(&mut self, f: impl FnOnce(&mut UserSettings)) -> Result<()> {
        f(&mut self.settings);
        self.save()
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

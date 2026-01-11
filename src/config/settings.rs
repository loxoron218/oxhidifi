//! User preference management with XDG Base Directory compliance.
//!
//! This module provides user settings management with proper XDG directory
//! usage for config and cache files.

use std::{
    env::var,
    fs::{create_dir_all, read_to_string, write},
    io::Error as StdError,
    path::PathBuf,
};

use {
    parking_lot::{RwLock, RwLockReadGuard},
    serde::{Deserialize, Serialize},
    serde_json::{Error as SerdeJsonError, from_str, to_string_pretty},
    thiserror::Error,
    tracing::debug,
};

/// Error type for settings operations.
#[derive(Error, Debug)]
pub enum SettingsError {
    /// Failed to read or write settings file.
    #[error("IO error: {0}")]
    IoError(#[from] StdError),
    /// Failed to serialize or deserialize settings.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] SerdeJsonError),
    /// Invalid settings value.
    #[error("Invalid settings value: {reason}")]
    InvalidValue { reason: String },
}

/// Serializable user settings structure with default values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserSettings {
    /// Audio output device name.
    pub audio_device: Option<String>,
    /// Sample rate for audio output (0 = auto).
    pub sample_rate: u32,
    /// Whether to use exclusive mode for bit-perfect playback.
    pub exclusive_mode: bool,
    /// Buffer duration in milliseconds.
    pub buffer_duration_ms: u32,
    /// Music library directories.
    pub library_directories: Vec<String>,
    /// Whether to show DR values on album covers.
    pub show_dr_values: bool,
    /// Current zoom level for grid view (0-4, where 0 is smallest).
    pub grid_zoom_level: u8,
    /// Current zoom level for list view (0-2, where 0 is smallest).
    pub list_zoom_level: u8,
    /// Theme preference (system/light/dark).
    pub theme_preference: String,
    /// Year display mode for albums ("release" or "original").
    pub year_display_mode: String,
    /// Whether to show metadata overlays (title, artist, format, year) on album cards.
    pub show_metadata_overlays: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            audio_device: None,
            sample_rate: 0, // Auto-detect
            exclusive_mode: true,
            buffer_duration_ms: 50,
            library_directories: vec![],
            show_dr_values: true,
            grid_zoom_level: 2, // Default medium zoom level (0-4)
            list_zoom_level: 1, // Default medium zoom level (0-2)
            theme_preference: "system".to_string(),
            year_display_mode: "release".to_string(), // Default to release year
            show_metadata_overlays: true,             // Default to showing metadata overlays
        }
    }
}

/// Handles loading, saving, and validation of user preferences.
#[derive(Debug)]
pub struct SettingsManager {
    /// Thread-safe user settings storage.
    settings: RwLock<UserSettings>,
    /// Path to the configuration file on disk.
    config_path: PathBuf,
}

impl Clone for SettingsManager {
    fn clone(&self) -> Self {
        Self {
            settings: RwLock::new(self.settings.read().clone()),
            config_path: self.config_path.clone(),
        }
    }
}

impl SettingsManager {
    /// Creates a new settings manager with default config path.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `SettingsManager` or a `SettingsError`.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be loaded from disk.
    pub fn new() -> Result<Self, SettingsError> {
        Self::with_config_path(get_config_path())
    }

    /// Creates a new settings manager with a custom config path (for testing).
    ///
    /// # Arguments
    ///
    /// * `config_path` - Custom path for the settings file
    ///
    /// # Returns
    ///
    /// A `Result` containing the `SettingsManager` or a `SettingsError`.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be loaded from disk.
    pub fn with_config_path(config_path: PathBuf) -> Result<Self, SettingsError> {
        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            create_dir_all(parent)?;
        }

        let settings = if config_path.exists() {
            debug!("Loading settings from existing file: {:?}", config_path);
            let contents = read_to_string(&config_path)?;
            from_str(&contents)?
        } else {
            debug!("Creating new default settings file: {:?}", config_path);
            UserSettings::default()
        };

        Ok(SettingsManager {
            settings: RwLock::new(settings),
            config_path,
        })
    }

    /// Gets the current settings.
    ///
    /// # Returns
    ///
    /// A reference to the current `UserSettings`.
    pub fn get_settings(&self) -> RwLockReadGuard<'_, UserSettings> {
        self.settings.read()
    }

    /// Gets the configuration file path.
    ///
    /// # Returns
    ///
    /// A reference to the configuration file path.
    pub fn get_config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Updates the settings and saves them to disk.
    ///
    /// # Arguments
    ///
    /// * `new_settings` - New settings to apply.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be saved to disk.
    pub fn update_settings(&self, new_settings: UserSettings) -> Result<(), SettingsError> {
        let mut settings_write = self.settings.write();
        *settings_write = new_settings;
        drop(settings_write);
        self.save_settings()
    }

    /// Saves the current settings to disk.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be saved to disk.
    fn save_settings(&self) -> Result<(), SettingsError> {
        debug!("Saving settings to file: {:?}", self.config_path);
        let contents = to_string_pretty(&*self.settings.read())?;
        write(&self.config_path, contents)?;
        Ok(())
    }
}

/// Ensures proper XDG directory usage for config and cache files.
///
/// # Returns
///
/// The path to the configuration file.
#[must_use]
pub fn get_config_path() -> PathBuf {
    let mut config_dir = get_xdg_config_home();
    config_dir.push("oxhidifi");
    config_dir.push("settings.json");
    config_dir
}

/// Gets the cache directory path.
///
/// # Returns
///
/// The path to the cache directory.
#[must_use]
pub fn get_cache_dir() -> PathBuf {
    let mut cache_dir = get_xdg_cache_home();
    cache_dir.push("oxhidifi");
    cache_dir
}

/// Gets the XDG config home directory following XDG Base Directory specification.
///
/// Uses `XDG_CONFIG_HOME` environment variable if set, otherwise defaults to $HOME/.config
fn get_xdg_config_home() -> PathBuf {
    if let Ok(config_home) = var("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return PathBuf::from(config_home);
    }

    if let Ok(home) = var("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        return path;
    }

    // Fallback to current directory if HOME is not set (shouldn't happen on Unix)
    PathBuf::from(".")
}

/// Gets the XDG cache home directory following XDG Base Directory specification.
///
/// Uses `XDG_CACHE_HOME` environment variable if set, otherwise defaults to $HOME/.cache
fn get_xdg_cache_home() -> PathBuf {
    if let Ok(cache_home) = var("XDG_CACHE_HOME")
        && !cache_home.is_empty()
    {
        return PathBuf::from(cache_home);
    }

    if let Ok(home) = var("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".cache");
        return path;
    }

    // Fallback to current directory if HOME is not set (shouldn't happen on Unix)
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind::NotFound};

    use serde_json::{from_str, to_string};

    use crate::config::settings::{SettingsError, UserSettings};

    #[test]
    fn test_user_settings_default() {
        let settings = UserSettings::default();
        assert_eq!(settings.sample_rate, 0);
        assert_eq!(settings.exclusive_mode, true);
        assert_eq!(settings.buffer_duration_ms, 50);
        assert_eq!(settings.show_dr_values, true);
        assert_eq!(settings.theme_preference, "system");
    }

    #[test]
    fn test_user_settings_serialization() {
        let settings = UserSettings {
            audio_device: Some("Test Device".to_string()),
            sample_rate: 96000,
            exclusive_mode: false,
            buffer_duration_ms: 100,
            library_directories: vec!["/music".to_string()],
            show_dr_values: false,
            grid_zoom_level: 2,
            list_zoom_level: 1,
            theme_preference: "dark".to_string(),
            year_display_mode: "original".to_string(),
            show_metadata_overlays: false,
        };

        let serialized = to_string(&settings).unwrap();
        let deserialized: UserSettings = from_str(&serialized).unwrap();
        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_settings_error_display() {
        let io_error = Error::new(NotFound, "File not found");
        let settings_error = SettingsError::IoError(io_error);
        assert!(settings_error.to_string().contains("IO error"));

        let invalid_value_error = SettingsError::InvalidValue {
            reason: "test reason".to_string(),
        };
        assert_eq!(
            invalid_value_error.to_string(),
            "Invalid settings value: test reason"
        );
    }
}

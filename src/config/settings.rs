//! User preference management with XDG Base Directory compliance.
//!
//! This module provides user settings management with proper XDG directory
//! usage for config and cache files.

use std::{
    env::var,
    fs::{canonicalize, create_dir_all, read_to_string, write},
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

/// Volume control mode - application or system volume.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum VolumeMode {
    /// Application-controlled volume.
    #[default]
    #[serde(rename = "app")]
    App,
    /// System-level volume control.
    #[serde(rename = "system")]
    System,
}

/// Sort order for column view.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum SortOrder {
    /// Ascending sort order.
    #[default]
    #[serde(rename = "ascending")]
    Ascending,
    /// Descending sort order.
    #[serde(rename = "descending")]
    Descending,
}

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
    /// Directory does not exist or is not accessible.
    #[error("Directory not found: {0}: {1}")]
    DirectoryNotFound(String, #[source] StdError),
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
    /// Debounce timeout in milliseconds for search input (default: 150).
    #[serde(default = "default_search_debounce_ms")]
    pub search_debounce_ms: u64,
    /// Volume control mode (application or system).
    pub volume_mode: VolumeMode,
    /// Column sort state for albums view (column title).
    #[serde(default)]
    pub albums_sort_column: Option<String>,
    /// Sort order for albums view.
    #[serde(default)]
    pub albums_sort_order: SortOrder,
    /// Column sort state for artists view (column title).
    #[serde(default)]
    pub artists_sort_column: Option<String>,
    /// Sort order for artists view.
    #[serde(default)]
    pub artists_sort_order: SortOrder,
}

/// Handles loading, saving, and validation of user preferences.
#[derive(Debug)]
pub struct SettingsManager {
    /// Thread-safe user settings storage.
    settings: RwLock<UserSettings>,
    /// Path to the configuration file on disk.
    config_path: PathBuf,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            audio_device: None,
            sample_rate: 0, // Auto-detect
            exclusive_mode: true,
            buffer_duration_ms: 500,
            library_directories: vec![],
            show_dr_values: true,
            grid_zoom_level: 2, // Default medium zoom level (0-4)
            list_zoom_level: 1, // Default medium zoom level (0-2)
            theme_preference: "system".to_string(),
            year_display_mode: "release".to_string(), // Default to release year
            show_metadata_overlays: true,             // Default to showing metadata overlays
            search_debounce_ms: 150,                  // Default debounce timeout for search
            volume_mode: VolumeMode::App,             // Default to application volume control
            albums_sort_column: None,                 // Default to no specific sort column
            albums_sort_order: SortOrder::Ascending,  // Default ascending order
            artists_sort_column: None,                // Default to no specific sort column
            artists_sort_order: SortOrder::Ascending, // Default ascending order
        }
    }
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

        Ok(Self {
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

    /// Gets only the `library_directories` field.
    ///
    /// # Returns
    ///
    /// A clone of the `library_directories` vector.
    pub fn get_library_directories(&self) -> Vec<String> {
        self.settings.read().library_directories.clone()
    }

    /// Gets settings needed for library scanning.
    ///
    /// # Returns
    ///
    /// A tuple of (`library_directories`, `show_dr_values`).
    pub fn get_scanner_settings(&self) -> (Vec<String>, bool) {
        let settings = self.settings.read();
        (
            settings.library_directories.clone(),
            settings.show_dr_values,
        )
    }

    /// Adds a library directory to settings.
    ///
    /// # Arguments
    ///
    /// * `directory` - The directory path to add.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be saved to disk.
    pub fn add_library_directory(&self, directory: &str) -> Result<(), SettingsError> {
        let canonical_dir = canonicalize(directory)
            .map_err(|e| SettingsError::DirectoryNotFound(directory.to_string(), e))?;
        let canonical_dir_string = canonical_dir.to_string_lossy().to_string();

        let settings = self.settings.read().clone();
        let is_duplicate = settings.library_directories.iter().any(|dir| {
            canonicalize(dir)
                .is_ok_and(|canonical| canonical.to_string_lossy() == canonical_dir_string)
        });

        if !is_duplicate {
            drop(settings);
            let mut settings = self.settings.read().clone();
            settings.library_directories.push(canonical_dir_string);
            *self.settings.write() = settings;
            return self.save_settings();
        }
        Ok(())
    }

    /// Removes a library directory from settings.
    ///
    /// # Arguments
    ///
    /// * `directory` - The directory path to remove.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be saved to disk.
    pub fn remove_library_directory(&self, directory: &str) -> Result<(), SettingsError> {
        let canonical_dir = canonicalize(directory)
            .map_err(|e| SettingsError::DirectoryNotFound(directory.to_string(), e))?;
        let canonical_dir_string = canonical_dir.to_string_lossy().to_string();

        let mut settings = self.settings.read().clone();
        settings
            .library_directories
            .retain(|dir| *dir != canonical_dir_string);
        *self.settings.write() = settings;
        self.save_settings()
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

/// Default debounce timeout for search input (150ms).
const fn default_search_debounce_ms() -> u64 {
    150
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
    use std::io::{Error as IoError, ErrorKind::NotFound};

    use {
        anyhow::{Result, bail},
        serde_json::{from_str, to_string},
    };

    use crate::config::settings::{
        SettingsError, SortOrder::Descending, UserSettings, VolumeMode::System,
    };

    #[test]
    fn test_user_settings_default() {
        let settings = UserSettings::default();
        assert_eq!(settings.sample_rate, 0);
        assert!(settings.exclusive_mode);
        assert_eq!(settings.buffer_duration_ms, 500);
        assert!(settings.show_dr_values);
        assert_eq!(settings.theme_preference, "system");
    }

    #[test]
    fn test_user_settings_serialization() -> Result<()> {
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
            search_debounce_ms: 150,
            volume_mode: System,
            albums_sort_column: Some("Album".to_string()),
            albums_sort_order: Descending,
            artists_sort_column: Some("Artist".to_string()),
            artists_sort_order: Descending,
        };

        let serialized = to_string(&settings)?;
        let deserialized: UserSettings = from_str(&serialized)?;
        if settings != deserialized {
            bail!("Expected equality after serialization roundtrip");
        }
        Ok(())
    }

    #[test]
    fn test_settings_error_display() -> Result<()> {
        let io_error = IoError::new(NotFound, "File not found");
        let settings_error = SettingsError::IoError(io_error);
        if !settings_error.to_string().contains("IO error") {
            bail!("Expected 'IO error' in error string, got '{settings_error}'");
        }

        let invalid_value_error = SettingsError::InvalidValue {
            reason: "Test reason".to_string(),
        };
        let error_string = invalid_value_error.to_string();
        if error_string != "Invalid settings value: Test reason" {
            bail!("Expected 'Invalid settings value: Test reason', got '{error_string}'");
        }
        Ok(())
    }
}

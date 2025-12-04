//! User preference management with XDG Base Directory compliance.
//!
//! This module provides user settings management with proper XDG directory
//! usage for config and cache files.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type for settings operations.
#[derive(Error, Debug)]
pub enum SettingsError {
    /// Failed to read or write settings file.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    /// Failed to serialize or deserialize settings.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
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
    /// Default view mode (grid or list).
    pub default_view_mode: String,
    /// Theme preference (system/light/dark).
    pub theme_preference: String,
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
            default_view_mode: "grid".to_string(),
            theme_preference: "system".to_string(),
        }
    }
}

/// Handles loading, saving, and validation of user preferences.
pub struct SettingsManager {
    settings: UserSettings,
    config_path: PathBuf,
}

impl SettingsManager {
    /// Creates a new settings manager.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `SettingsManager` or a `SettingsError`.
    ///
    /// # Errors
    ///
    /// Returns `SettingsError` if settings cannot be loaded from disk.
    pub fn new() -> Result<Self, SettingsError> {
        let config_path = get_config_path();
        
        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let settings = if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            serde_json::from_str(&contents)?
        } else {
            UserSettings::default()
        };

        Ok(SettingsManager {
            settings,
            config_path,
        })
    }

    /// Gets the current settings.
    ///
    /// # Returns
    ///
    /// A reference to the current `UserSettings`.
    pub fn get_settings(&self) -> &UserSettings {
        &self.settings
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
    pub fn update_settings(&mut self, new_settings: UserSettings) -> Result<(), SettingsError> {
        self.settings = new_settings;
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
        let contents = serde_json::to_string_pretty(&self.settings)?;
        std::fs::write(&self.config_path, contents)?;
        Ok(())
    }
}

/// Ensures proper XDG directory usage for config and cache files.
///
/// # Returns
/// 
/// The path to the configuration file.
pub fn get_config_path() -> PathBuf {
    let mut config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    config_dir.push("oxhidifi");
    config_dir.push("settings.json");
    config_dir
}

/// Gets the cache directory path.
///
/// # Returns
///
/// The path to the cache directory.
pub fn get_cache_dir() -> PathBuf {
    let mut cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    cache_dir.push("oxhidifi");
    cache_dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_settings_default() {
        let settings = UserSettings::default();
        assert_eq!(settings.sample_rate, 0);
        assert_eq!(settings.exclusive_mode, true);
        assert_eq!(settings.buffer_duration_ms, 50);
        assert_eq!(settings.show_dr_values, true);
        assert_eq!(settings.default_view_mode, "grid");
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
            default_view_mode: "list".to_string(),
            theme_preference: "dark".to_string(),
        };

        let serialized = serde_json::to_string(&settings).unwrap();
        let deserialized: UserSettings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_settings_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let settings_error = SettingsError::IoError(io_error);
        assert!(settings_error.to_string().contains("IO error"));

        let invalid_value_error = SettingsError::InvalidValue { 
            reason: "test reason".to_string() 
        };
        assert_eq!(invalid_value_error.to_string(), "Invalid settings value: test reason");
    }
}
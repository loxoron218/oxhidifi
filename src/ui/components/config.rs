use std::{
    collections::HashMap,
    env::var_os,
    fs::{File, create_dir_all, read_to_string},
    io::{Error, ErrorKind::Other, Result, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string_pretty};

use crate::ui::components::sorting::sorting_types::SortOrder;

/// Manages application settings, including sorting preferences and completed albums.
///
/// This module provides functionality to load and save user preferences to a JSON file.
/// The `Settings` struct defines the structure of these preferences.
///
/// The settings file is located in the user's platform-specific configuration directory
/// (e.g., `~/.config/oxhidifi/settings.json` on Linux).
///
/// # Examples
///
/// ```no_run
/// use crate::ui::components::config::{load_settings, save_settings, Settings};
///
/// let mut settings = load_settings();
/// settings.sort_ascending_albums = false;
/// save_settings(&settings).expect("Failed to save settings");
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Defines the order in which albums and artists should be sorted.
    pub sort_orders: Vec<SortOrder>,
    /// Indicates whether albums should be sorted in ascending order.
    pub sort_ascending_albums: bool,
    /// Indicates whether artists should be sorted in ascending order.
    pub sort_ascending_artists: bool,
    /// A map of completed album IDs to a boolean indicating their completion status.
    /// The `i64` key represents the album ID.
    pub completed_albums: HashMap<i64, bool>,
}

/// Provides default values for `Settings`.
///
/// This implementation ensures that if no settings file is found or if it's corrupt,
/// the application can start with a sensible default configuration.
impl Default for Settings {
    fn default() -> Self {
        Self {
            sort_orders: vec![
                SortOrder::Artist,
                SortOrder::Year,
                SortOrder::Album,
                SortOrder::Format,
            ],
            sort_ascending_albums: true,
            sort_ascending_artists: true,
            completed_albums: HashMap::new(),
        }
    }
}

/// Determines the path to the application's configuration directory and creates it if it doesn't exist.
///
/// This function attempts to find the user's home directory using `HOME` (Unix) or `USERPROFILE` (Windows).
/// If neither is found, it defaults to the current directory.
/// It then constructs the path to `.config/oxhidifi` within the base directory.
///
/// # Returns
///
/// A `Result` which is:
/// - `Ok(PathBuf)`: The path to the configuration directory if successful.
/// - `Err(io::Error)`: If the directory cannot be created.
fn get_config_dir() -> Result<PathBuf> {
    let base_dir = var_os("HOME")
        .or_else(|| var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".")); // Fallback to current directory if home not found

    let config_dir = base_dir.join(".config").join("oxhidifi");
    create_dir_all(&config_dir)?; // Create directory if it doesn't exist
    Ok(config_dir)
}

/// Returns the full path to the `settings.json` file.
///
/// This function relies on `get_config_dir` to determine the base configuration directory
/// and appends "settings.json" to it.
///
/// # Returns
///
/// A `Result` which is:
/// - `Ok(PathBuf)`: The full path to the settings file.
/// - `Err(io::Error)`: If the configuration directory cannot be determined or created.
fn get_settings_path() -> Result<PathBuf> {
    let mut path = get_config_dir()?;
    path.push("settings.json");
    Ok(path)
}

/// Loads application settings from the `settings.json` file on disk.
///
/// If the file does not exist, is unreadable, or contains invalid JSON,
/// default settings are returned. Errors during file operations or deserialization
/// are logged to the console.
///
/// # Returns
///
/// The loaded `Settings` struct, or `Settings::default()` if loading fails.
pub fn load_settings() -> Settings {
    let settings_path = match get_settings_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to get settings path: {}", e);
            let default_settings = Settings::default();
            if let Err(save_err) = save_settings(&default_settings) {
                eprintln!("Failed to save default settings: {}", save_err);
            }
            return default_settings;
        }
    };

    match read_to_string(&settings_path) {
        Ok(data) => match from_str(&data) {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("Failed to parse settings from {}: {}", settings_path.display(), e);
                let default_settings = Settings::default();
                if let Err(save_err) = save_settings(&default_settings) {
                    eprintln!("Failed to save default settings: {}", save_err);
                }
                default_settings
            }
        },
        Err(e) => {
            eprintln!("Failed to read settings file {}: {}", settings_path.display(), e);
            let default_settings = Settings::default();
            if let Err(save_err) = save_settings(&default_settings) {
                eprintln!("Failed to save default settings: {}", save_err);
            }
            default_settings
        }
    }
}

/// Saves the current application settings to the `settings.json` file on disk.
///
/// The settings are serialized to a pretty-printed JSON format.
///
/// # Arguments
///
/// * `settings` - A reference to the `Settings` struct to be saved.
///
/// # Returns
///
/// A `Result` which is:
/// - `Ok(())`: If the settings were successfully saved.
/// - `Err(io::Error)`: If there was an error during serialization, file creation, or writing.
pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = get_settings_path()?;
    let data = to_string_pretty(settings)
        .map_err(|e| Error::new(Other, format!("Failed to serialize settings: {}", e)))?;

    let mut file = File::create(&path)?;
    file.write_all(data.as_bytes())?;
    Ok(())
}

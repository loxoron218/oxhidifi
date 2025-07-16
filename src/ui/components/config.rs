use std::{env::var_os, path::PathBuf}; 
use std::fs::{create_dir_all, File, read_to_string};
use std::io::{self, Write}; 

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string_pretty};

use crate::ui::components::sorting::SortOrder;

/// Settings Struct & Default
#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub sort_orders: Vec<SortOrder>,
    pub sort_ascending_albums: bool,
    pub sort_ascending_artists: bool,
}

/// Provides default values for Settings (default sort order and ascending state).
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
        }
    }
}

/// Returns the path to the application's settings file, creating the config directory if needed.
fn config_path() -> PathBuf {
    let base_dir = var_os("HOME")
        .or_else(|| var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut path = base_dir.join(".config/oxhidifi");
    create_dir_all(&path).ok();
    path.push("settings.json");
    path
}

/// Loads settings from disk, or returns defaults if missing/corrupt.
pub fn load_settings() -> Settings {
    let path = config_path();
    if let Ok(data) = read_to_string(&path) {
        from_str(&data).unwrap_or_default()
    } else {
        Settings::default()
    }
}

/// Saves settings to disk as pretty-printed JSON.
pub fn save_settings(settings: &Settings) -> io::Result<()> {
    let path = config_path();
    let data = to_string_pretty(settings).unwrap();
    let mut file = File::create(path)?;
    file.write_all(data.as_bytes())
}

use std::{
    collections::HashMap,
    env::var,
    fs::{File, create_dir_all, read_to_string},
    io::{Result as IoResult, Write},
    path::PathBuf,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use serde_json::{from_str, to_string_pretty};

/// Specifies the directory name for configuration files relative to the user's home directory.
const CONFIG_DIR: &str = ".config/oxhidifi";

/// Defines the filename for storing the best DR values in JSON format.
const DR_VALUES_FILE: &str = "best_dr_values.json";

/// Represents a unique key for an album, used for storing and retrieving its DR value.
///
/// This struct is designed to be used as a key in a `HashMap`. Its `Serialize` and
/// `Deserialize` implementations convert it to/from a single string to enable
/// `serde_json` to use it as a map key in the persisted JSON file.
///
/// The string format uses specific delimiters to combine `title`, `artist`, and
/// `folder_path`. This approach is taken to allow `AlbumKey` to function as a
/// `HashMap` key when serialized, as `serde_json` typically requires string keys.
///
/// **Warning**: If album `title`, `artist`, or `folder_path` contain the delimiters
/// `<<<DRKEY>>>` or `<<<DRPART>>>`, deserialization will fail.
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct AlbumKey {
    pub title: String,
    pub artist: String,
    pub folder_path: PathBuf,
}

/// Custom serialization for `AlbumKey`.
///
/// Concatenates the struct fields into a single string using unique separators.
impl Serialize for AlbumKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let key_string = format!(
            "{}<<<DRKEY>>>{}<<<DRPART>>>{}",
            self.title,
            self.artist,
            self.folder_path.to_str().unwrap_or_default()
        );
        serializer.serialize_str(&key_string)
    }
}

/// Custom deserialization for `AlbumKey`.
///
/// Splits the serialized string back into its constituent fields.
/// Handles potential parsing errors by returning a `serde::de::Error`.
impl<'de> Deserialize<'de> for AlbumKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.split("<<<DRKEY>>>").collect();
        if parts.len() != 2 {
            return Err(Error::custom(format!(
                "Invalid AlbumKey format: expected 2 parts separated by '<<<DRKEY>>>', got {} in '{}'",
                parts.len(),
                s
            )));
        }
        let title = parts[0].to_string();
        let sub_parts: Vec<&str> = parts[1].split("<<<DRPART>>>").collect();
        if sub_parts.len() != 2 {
            return Err(Error::custom(format!(
                "Invalid AlbumKey format: expected 2 sub-parts separated by '<<<DRPART>>>', got {} in '{}'",
                sub_parts.len(),
                parts[1]
            )));
        }
        let artist = sub_parts[0].to_string();
        let folder_path = PathBuf::from(sub_parts[1]);
        Ok(AlbumKey {
            title,
            artist,
            folder_path,
        })
    }
}

/// A struct for storing and managing the best DR (Dynamic Range) values for albums.
///
/// It uses a `HashMap` where the key is an `AlbumKey` and the value is the
/// DR value (u8). Provides methods for loading from and saving to a JSON file,
/// as well as adding, removing, and checking for the existence of best DR values.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DrValueStore {
    /// A hash map storing `AlbumKey` to best DR value (u8) mappings.
    pub dr_values: HashMap<AlbumKey, u8>,
}
impl DrValueStore {
    /// Returns the full `PathBuf` to the configuration directory where DR values are stored.
    ///
    /// This path is constructed by joining the user's home directory with `CONFIG_DIR`.
    /// Panics if the `HOME` environment variable is not set.
    fn config_dir_path() -> PathBuf {
        let home_dir = var("HOME").expect("HOME environment variable not set");
        PathBuf::from(home_dir).join(CONFIG_DIR)
    }

    /// Returns the full `PathBuf` to the JSON file where DR values are persisted.
    ///
    /// This path is constructed by joining the configuration directory path with `DR_VALUES_FILE`.
    fn dr_values_file_path() -> PathBuf {
        Self::config_dir_path().join(DR_VALUES_FILE)
    }

    /// Loads the `DrValueStore` from the persisted JSON file.
    ///
    /// If the file does not exist, or if there is an error reading or parsing the file,
    /// an empty `DrValueStore` (default instance) is returned. Errors are silently
    /// handled to ensure the application can always proceed with a default store.
    pub fn load() -> Self {
        let file_path = Self::dr_values_file_path();
        if !file_path.exists() {
            return Self::default();
        }
        match read_to_string(&file_path) {
            Ok(contents) => from_str(&contents).unwrap_or_else(|e| {
                eprintln!("Error parsing DR values from JSON: {}", e);
                Self::default()
            }),
            Err(e) => {
                eprintln!("Error reading DR values file: {}", e);
                Self::default()
            }
        }
    }

    /// Saves the current `DrValueStore` to the JSON file.
    ///
    /// The configuration directory is created if it does not already exist.
    /// The data is written in a pretty-printed JSON format.
    ///
    /// Returns `Ok(())` on success, or an `std::io::Error` if any file
    /// system or serialization operation fails.
    pub fn save(&self) -> IoResult<()> {
        let config_dir = Self::config_dir_path();

        // Ensure the configuration directory exists
        create_dir_all(&config_dir)?;
        let file_path = Self::dr_values_file_path();

        // Serialize to pretty-printed JSON
        let json_contents = to_string_pretty(&self)?;

        // Create or overwrite the file
        let mut file = File::create(&file_path)?;

        // Write the JSON contents
        file.write_all(json_contents.as_bytes())?;
        Ok(())
    }

    /// Adds or updates a best DR value for a given `AlbumKey`.
    ///
    /// If the `key` already exists, its associated best DR value will be updated.
    pub fn add_dr_value(&mut self, key: AlbumKey, dr_value: u8) {
        self.dr_values.insert(key, dr_value);
    }

    /// Removes a best DR value associated with a given `AlbumKey`.
    ///
    /// If the `key` does not exist in the store, this operation does nothing.
    pub fn remove_dr_value(&mut self, key: &AlbumKey) {
        self.dr_values.remove(key);
    }

    /// Checks if a best DR value exists for a given `AlbumKey`.
    ///
    /// Returns `true` if the `key` is present in the store, `false` otherwise.
    pub fn contains(&self, key: &AlbumKey) -> bool {
        self.dr_values.contains_key(key)
    }
}

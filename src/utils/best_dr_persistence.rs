use std::fs::{File, create_dir_all, read_to_string};
use std::io::{Result as stdResult, Write};
use std::{collections::HashMap, env::var, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use serde_json::{from_str, to_string_pretty};

// Specifies the directory name for configuration files.
const CONFIG_DIR: &str = ".config/oxhidifi";

// Defines the filename for storing the best DR values in JSON format.
const DR_VALUES_FILE: &str = "best_dr_values.json";

/// Represents a unique key for an album.
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct AlbumKey {
    pub title: String,
    pub artist: String,
    pub folder_path: String,
}

/// Custom serialization for `AlbumKey`.
impl Serialize for AlbumKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Concatenate the struct fields into a single string with unique separators.
        let key_string = format!(
            "{}<<<DRKEY>>>{}<<<DRPART>>>{}",
            self.title, self.artist, self.folder_path
        );
        serializer.serialize_str(&key_string)
    }
}

/// Custom deserialization for `AlbumKey`.
impl<'de> Deserialize<'de> for AlbumKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize the string from the input.
        let s = String::deserialize(deserializer)?;

        // Split the string into two parts at the first separator.
        let parts: Vec<&str> = s.split("<<<DRKEY>>>").collect();
        if parts.len() == 2 {
            let title = parts[0].to_string();

            // Split the second part to get the artist and folder path.
            let sub_parts: Vec<&str> = parts[1].split("<<<DRPART>>>").collect();
            if sub_parts.len() == 2 {
                let artist = sub_parts[0].to_string();
                let folder_path = sub_parts[1].to_string();

                // Return the successfully reconstructed AlbumKey.
                Ok(AlbumKey {
                    title,
                    artist,
                    folder_path,
                })
            } else {
                // Return an error if the second part does not have the correct format.
                Err(Error::custom("Invalid AlbumKey format"))
            }
        } else {
            // Return an error if the string does not have the correct format.
            Err(Error::custom("Invalid AlbumKey format"))
        }
    }
}

/// A struct for storing DR (Dynamic Range) values.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DrValueStore {
    // HashMap where key is AlbumKey and value is the DR value (u8)
    pub dr_values: HashMap<AlbumKey, u8>,
}

// This block contains the implementation of methods for the `DrValueStore` struct.
impl DrValueStore {
    /// Returns the full path to the configuration directory.
    fn config_dir_path() -> PathBuf {
        let home_dir = var("HOME").expect("HOME environment variable not set");
        PathBuf::from(home_dir).join(CONFIG_DIR)
    }

    /// Returns the full path to the DR values JSON file.
    fn dr_values_file_path() -> PathBuf {
        Self::config_dir_path().join(DR_VALUES_FILE)
    }

    /// Loads DR values from the JSON file.
    pub fn load() -> Self {
        let file_path = Self::dr_values_file_path();
        if file_path.exists() {
            match read_to_string(&file_path) {
                Ok(contents) => match from_str(&contents) {
                    Ok(store) => store,
                    Err(_e) => Self::default(),
                },
                Err(_e) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    /// Saves DR values to the JSON file.
    pub fn save(&self) -> stdResult<()> {
        // Specify io::Result
        let config_dir = Self::config_dir_path();
        create_dir_all(&config_dir)?;
        let file_path = Self::dr_values_file_path();
        let json_contents = to_string_pretty(&self)?;
        let mut file = File::create(&file_path)?;
        file.write_all(json_contents.as_bytes())?;
        Ok(())
    }

    /// Adds or updates a DR value for a given album key.
    pub fn add_dr_value(&mut self, key: AlbumKey, dr_value: u8) {
        self.dr_values.insert(key, dr_value);
    }

    /// Removes a DR value for a given album key.
    pub fn remove_dr_value(&mut self, key: &AlbumKey) {
        self.dr_values.remove(key);
    }

    /// Checks if a DR value exists for a given album key.
    pub fn contains(&self, key: &AlbumKey) -> bool {
        self.dr_values.contains_key(key)
    }
}

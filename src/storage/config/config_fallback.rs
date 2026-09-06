//! Corruption fallback and directory helpers for settings persistence.
//!
//! Extracted from `persistence.rs` to keep each file under 400 lines.

use std::{io::Error, path::Path};

use {
    serde_json::from_str,
    tokio::fs::{create_dir_all, read_to_string, rename, try_exists},
    tracing::warn,
};

use crate::storage::{
    StorageError::{self, Database},
    StorageResult,
    settings::UserSettings,
};

/// Format a config directory creation error.
#[must_use]
pub fn dir_error(dir: &Path, err: &Error) -> StorageError {
    Database(format!("{}: {err}", dir.display()))
}

/// Ensure the parent directory exists.
///
/// # Errors
///
/// Returns an error if the directory cannot be created.
pub async fn ensure_parent_dir(dir: &Path) -> StorageResult<()> {
    create_dir_all(dir).await.map_err(|e| dir_error(dir, &e))?;
    Ok(())
}

/// Try to load settings from file, falling back to defaults on parse error.
///
/// # Errors
///
/// Returns an error if the settings file exists but cannot be read.
pub async fn load_settings_with_fallback(settings_path: &Path) -> StorageResult<UserSettings> {
    if try_exists(settings_path).await.unwrap_or(false) {
        let content = read_to_string(settings_path).await.map_err(|e| {
            Database(format!(
                "Failed to read settings: {}: {e}",
                settings_path.display()
            ))
        })?;
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
pub async fn backup_corrupt_settings(settings_path: &Path) {
    let backup_path = settings_path.with_extension("json.corrupt");
    if let Err(e) = rename(settings_path, &backup_path).await {
        warn!(
            error = %e,
            path = %backup_path.display(),
            "Failed to back up corrupt settings file",
        );
    }
}

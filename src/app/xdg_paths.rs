//! XDG base directory resolution.

use std::{
    env::{VarError, var, var_os},
    path::PathBuf,
};

use thiserror::Error;

/// Error type for XDG base directory resolution.
#[derive(Debug, Error)]
pub enum XdgError {
    /// `HOME` environment variable is not set.
    #[error("HOME environment variable is not set: {0}")]
    MissingHome(#[from] VarError),
}

/// Convenience alias for XDG directory resolution results.
pub type XdgResult<T> = Result<T, XdgError>;

/// Resolve an XDG directory from an environment variable with a fallback path.
fn resolve_xdg_dir(env_var: &str, fallback: &str) -> XdgResult<PathBuf> {
    if let Some(dir) = var_os(env_var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
    {
        return Ok(dir);
    }
    let home = var("HOME")?;
    Ok(PathBuf::from(home).join(fallback))
}

/// Resolve the XDG data home directory.
///
/// Falls back to `$HOME/.local/share` when `XDG_DATA_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_DATA_HOME` is also unset.
pub fn dirs_data_home() -> XdgResult<PathBuf> {
    resolve_xdg_dir("XDG_DATA_HOME", ".local/share")
}

/// Resolve the XDG config home directory.
///
/// Falls back to `$HOME/.config` when `XDG_CONFIG_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_CONFIG_HOME` is also unset.
pub fn dirs_config_home() -> XdgResult<PathBuf> {
    resolve_xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Resolve the XDG cache home directory.
///
/// Falls back to `$HOME/.cache` when `XDG_CACHE_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_CACHE_HOME` is also unset.
pub fn dirs_cache_home() -> XdgResult<PathBuf> {
    resolve_xdg_dir("XDG_CACHE_HOME", ".cache")
}

/// Build the data directory for the application database.
#[must_use]
pub fn data_dir() -> PathBuf {
    dirs_data_home()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("oxhidifi")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::xdg_paths::{data_dir, dirs_config_home, dirs_data_home};

    #[test]
    fn xdg_data_home_resolves_to_absolute_path() {
        let dir = dirs_data_home().unwrap_or_else(|_| PathBuf::from("."));
        assert!(dir.is_absolute());
    }

    #[test]
    fn xdg_config_home_resolves_to_absolute_path() {
        let dir = dirs_config_home().unwrap_or_else(|_| PathBuf::from("."));
        assert!(dir.is_absolute());
    }

    #[test]
    fn data_dir_appends_application_component() {
        let dir = data_dir();
        assert!(dir.ends_with("oxhidifi"));
        assert!(dir.is_absolute());
    }
}

//! Application-level utilities including XDG base directory resolution and
//! Libadwaita `AdwApplication` setup.

use std::{
    env::{var, var_os},
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
};

use {
    anyhow::{Context, Result},
    libadwaita::{Application, prelude::*},
    tracing::info,
};

use crate::{
    playback::engine::PlaybackEngine, storage::database::SqliteStorage, ui::window::build_window,
};

/// Application identifier for D-Bus and resource paths.
const APP_ID: &str = "com.github.oxhidifi";

/// Shared application state passed to the window.
pub struct AppState {
    /// The playback engine controlling audio output.
    pub playback: Arc<PlaybackEngine>,
    /// The storage backend for library data.
    pub storage: Arc<SqliteStorage>,
}

/// Resolve an XDG directory from an environment variable with a fallback path.
///
/// # Errors
///
/// Returns an error if `HOME` environment variable is not set.
fn resolve_xdg_dir(env_var: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(dir) = var_os(env_var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
    {
        return Ok(dir);
    }
    let home = var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(fallback))
}

/// Resolve the XDG data home directory.
///
/// Falls back to `$HOME/.local/share` when `XDG_DATA_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_DATA_HOME` is also unset.
pub fn dirs_data_home() -> Result<PathBuf> {
    resolve_xdg_dir("XDG_DATA_HOME", ".local/share")
}

/// Resolve the XDG config home directory.
///
/// Falls back to `$HOME/.config` when `XDG_CONFIG_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_CONFIG_HOME` is also unset.
pub fn dirs_config_home() -> Result<PathBuf> {
    resolve_xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Resolve the XDG cache home directory.
///
/// Falls back to `$HOME/.cache` when `XDG_CACHE_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_CACHE_HOME` is also unset.
pub fn dirs_cache_home() -> Result<PathBuf> {
    resolve_xdg_dir("XDG_CACHE_HOME", ".cache")
}

/// Build the data directory for the application database.
fn data_dir() -> PathBuf {
    dirs_data_home()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("oxhidifi")
}

/// Build and run the Libadwaita application.
///
/// Initializes the storage backend, playback engine, and presents the main
/// window. This is the top-level entry point for the GUI.
///
/// # Errors
///
/// Returns an error if the application cannot be built or if the storage
/// backend fails to initialize.
pub async fn run_application() -> Result<()> {
    let db_dir = data_dir();
    create_dir_all(&db_dir)
        .with_context(|| format!("Failed to create data directory: {}", db_dir.display()))?;

    let db_path = db_dir.join("library.db");
    let storage = Arc::new(
        SqliteStorage::connect(&db_path)
            .await
            .context("Failed to initialize storage")?,
    );

    let playback = Arc::new(PlaybackEngine::new());

    let state = Arc::new(AppState { playback, storage });

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        let window = build_window(app, &state);
        window.present();
    });

    info!("Starting application");
    app.run();

    Ok(())
}

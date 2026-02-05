//! Database connection utilities for the music library.
//!
//! This module provides database URL configuration and connection pool creation.

use std::{env::var, fs::create_dir_all, path::PathBuf};

use {
    sqlx::{
        Error, SqlitePool,
        sqlite::{SqliteConnectOptions, SqliteJournalMode::Wal, SqliteSynchronous::Normal},
    },
    tracing::warn,
};

/// Gets the database connection string following XDG Base Directory specification.
///
/// # Returns
///
/// The database connection string.
#[must_use]
pub fn get_database_url() -> String {
    let mut config_dir = get_xdg_config_home();
    config_dir.push("oxhidifi");
    config_dir.push("library.db");

    // Create parent directories if they don't exist
    if let Some(parent) = config_dir.parent()
        && let Err(e) = create_dir_all(parent)
    {
        warn!(
            "Failed to create config directory '{}': {}",
            parent.display(),
            e
        );
    }

    format!("sqlite://{}", config_dir.to_string_lossy())
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

/// Creates a database connection pool.
///
/// # Returns
///
/// A `Result` containing the connection pool.
///
/// # Errors
///
/// Returns `sqlx::Error` if connection pool creation fails.
pub async fn create_connection_pool() -> Result<SqlitePool, Error> {
    let database_url = get_database_url();

    let options = SqliteConnectOptions::new()
        .filename(database_url.replace("sqlite://", ""))
        .create_if_missing(true)
        .journal_mode(Wal)
        .synchronous(Normal);

    let pool = SqlitePool::connect_with(options).await?;
    Ok(pool)
}

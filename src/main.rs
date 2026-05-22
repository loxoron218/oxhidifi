//! Application entry point with structured logging initialization.

use std::{
    env::{var, var_os},
    fs::create_dir_all,
    io::stderr,
    path::PathBuf,
};

use {
    anyhow::{Context, Result},
    tracing::info,
    tracing_appender::{non_blocking, rolling::daily},
    tracing_subscriber::{
        filter::EnvFilter, fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt,
    },
};

/// Initialize structured logging to file and stderr.
///
/// # Errors
///
/// Returns an error if the log directory cannot be created or the HOME
/// environment variable is not set.
fn init_logging() -> Result<()> {
    let log_dir = dirs_data_home()?.join("oxhidifi");
    create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

    let file_appender = daily(log_dir, "oxhidifi.log");
    let (non_blocking, _guard) = non_blocking(file_appender);

    let file_layer = layer()
        .json()
        .with_writer(non_blocking)
        .with_target(true)
        .with_thread_ids(true);

    let stderr_layer = layer()
        .with_writer(stderr)
        .with_target(false)
        .with_thread_ids(false);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();

    Ok(())
}

/// Resolve the XDG data home directory.
///
/// Falls back to `$HOME/.local/share` when `XDG_DATA_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_DATA_HOME` is also unset.
fn dirs_data_home() -> Result<PathBuf> {
    if let Some(dir) = var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
    {
        return Ok(dir);
    }
    let home = var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".local").join("share"))
}

/// Application entry point.
///
/// Initializes logging and starts the application.
///
/// # Errors
///
/// Returns an error if logging initialization fails.
fn main() -> Result<()> {
    init_logging()?;
    info!("Application starting");
    Ok(())
}

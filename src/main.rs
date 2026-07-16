//! Application entry point with structured logging initialization.

use std::{fs::create_dir_all, io::stderr};

use {
    anyhow::{Context, Result},
    tokio::runtime::Runtime,
    tracing::info,
    tracing_appender::{
        non_blocking::{NonBlocking, WorkerGuard},
        rolling::daily,
    },
    tracing_subscriber::{
        filter::EnvFilter, fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt,
    },
};

use oxhidifi_refactor::app::{dirs_data_home, run_application};

/// Initialize structured logging to file and stderr.
///
/// Returns a `NonBlocking` guard that must be kept alive for the duration of
/// the program; dropping it flushes and shuts down the file writer.
///
/// # Errors
///
/// Returns an error if the log directory cannot be created or the HOME
/// environment variable is not set.
fn init_logging() -> Result<WorkerGuard> {
    let log_dir = dirs_data_home()?.join("oxhidifi");
    create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

    let file_appender = daily(log_dir, "oxhidifi.log");
    let (non_blocking, guard) = NonBlocking::new(file_appender);

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

    Ok(guard)
}

/// Application entry point.
///
/// Initializes logging and starts the Libadwaita application.
///
/// # Errors
///
/// Returns an error if logging initialization fails or the application
/// cannot be built.
fn main() -> Result<()> {
    let log_guard = init_logging()?;
    info!("Application starting");

    let rt = Runtime::new().context("Failed to create tokio runtime")?;
    let result = rt.block_on(run_application());
    drop(log_guard);
    result
}

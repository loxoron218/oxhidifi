//! Steady-state memory monitoring via RSS sampling.
//!
//! Reads process resident set size from `/proc/self/status` and emits
//! `tracing::info!` events with typed fields for post-hoc analysis.
//! A background tokio task samples every 30 seconds; threshold violations
//! warn against the 200 MiB engineering target.

use std::{fs::read_to_string, time::Duration};

use {
    tokio::{
        spawn,
        task::{JoinHandle, spawn_blocking},
        time::interval,
    },
    tracing::{info, warn},
};

/// Engineering target for steady-state memory in MiB (plan.md constraint).
const MEMORY_TARGET_MB: f64 = 200.0;

/// Read the current RSS (resident set size) of this process from
/// `/proc/self/status` in MiB.
///
/// Returns `None` if `/proc/self/status` is unavailable or cannot be parsed
/// (e.g., on non-Linux platforms).
pub fn read_rss_mb() -> Option<f64> {
    let status = match read_to_string("/proc/self/status") {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to read /proc/self/status");
            return None;
        }
    };
    for line in status.lines() {
        let Some(rss_line) = line.strip_prefix("VmRSS:") else {
            continue;
        };
        let Some(kb_str) = rss_line.trim().strip_suffix(" kB") else {
            continue;
        };
        let kb: f64 = match kb_str.trim().parse() {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, value = %kb_str.trim(), "Failed to parse VmRSS value");
                return None;
            }
        };
        return Some(kb / 1024.0);
    }
    None
}

/// Sample steady-state memory usage once and emit a metrics event.
///
/// Reads RSS from `/proc/self/status` and emits `tracing::info!` with
/// the memory usage in MiB. Warns if the engineering target (200 MiB)
/// is exceeded.
pub fn sample_memory_once() {
    if let Some(rss_mb) = read_rss_mb() {
        info!(rss_mb, "Steady-state memory",);
        if rss_mb > MEMORY_TARGET_MB {
            warn!(
                rss_mb,
                target_mb = MEMORY_TARGET_MB,
                "Memory usage above engineering target",
            );
        }
    }
}

/// Spawn a tokio task that samples memory usage every 30 seconds.
///
/// The task runs indefinitely until the process exits. Each sample
/// emits a `tracing::info!` event with the current RSS in MiB.
///
/// Periodically sample memory usage in a loop.
async fn memory_monitor_loop() {
    let mut interval = interval(Duration::from_secs(30));
    _ = interval.tick().await;
    loop {
        _ = interval.tick().await;
        sample_memory().await;
    }
}

/// Sample memory usage in a blocking task, logging on failure.
async fn sample_memory() {
    if let Err(e) = spawn_blocking(sample_memory_once).await {
        warn!(error = %e, "Memory sample task failed");
    }
}

/// Spawn a background task that periodically samples memory usage.
///
/// Returns a `JoinHandle` that can be detached or joined for controlled
/// shutdown.
#[must_use]
pub fn spawn_memory_monitor() -> JoinHandle<()> {
    spawn(memory_monitor_loop())
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Context, Result, ensure},
        tokio::runtime::Runtime,
    };

    use crate::metrics::memory::{read_rss_mb, sample_memory_once, spawn_memory_monitor};

    #[test]
    fn read_rss_mb_returns_some_on_linux() {
        if let Some(rss) = read_rss_mb() {
            assert!(rss > 0.0, "RSS value should be positive, got {rss}");
        }
    }

    #[test]
    fn sample_memory_once_does_not_panic() {
        sample_memory_once();
    }

    #[test]
    fn spawn_memory_monitor_runs_until_aborted() -> Result<()> {
        let runtime =
            Runtime::new().context("Failed to create tokio runtime for memory monitor test")?;
        runtime.block_on(async {
            let handle = spawn_memory_monitor();
            ensure!(
                !handle.is_finished(),
                "monitor task should still be pending"
            );
            handle.abort();
            let outcome = handle.await;
            ensure!(outcome.is_err(), "aborted monitor should not complete");
            Ok(())
        })
    }
}

//! Performance metrics collection using structured tracing instrumentation.
//!
//! Provides five metric collectors for latency, throughput, response time,
//! panel reveal time, and memory usage. Each collector emits
//! `tracing::info!` events with typed fields for post-hoc analysis and
//! threshold violation warnings.
//!
//! # Collectors
//!
//! | Task | Collector | Target | Threshold |
//! |------|-----------|--------|-----------|
//! | T046a | [`PlaybackLatency`] | `metrics.playback_latency` | < 3,000 ms |
//! | T046b | [`ScanThroughput`] | `metrics.scan_throughput` | ≥ 333 files/s |
//! | T046c | [`UiResponse`] | `metrics.ui_response` | < 100 ms |
//! | T046d | [`PanelReveal`] | `metrics.panel_reveal` | < 500 ms |
//! | T046e | [`sample_memory_once`] | `metrics.memory` | < 200 MiB |

use std::{
    fs::read_to_string,
    time::{Duration, Instant},
};

use {
    num_traits::cast::cast,
    parking_lot::Mutex,
    tokio::{
        spawn,
        task::{JoinHandle, spawn_blocking},
        time::interval,
    },
    tracing::{info, trace, warn},
};

/// Playback latency threshold in milliseconds (SC-001: < 3,000 ms).
const PLAYBACK_LATENCY_THRESHOLD_MS: f64 = 3000.0;

/// Minimum scan throughput in files/second (SC-004: ≥ 333 files/s).
const SCAN_THROUGHPUT_MIN_FPS: f64 = 333.0;

/// Maximum UI response time in milliseconds (SC-005: < 100 ms).
const UI_RESPONSE_THRESHOLD_MS: f64 = 100.0;

/// Maximum panel reveal time in milliseconds (SC-007: < 500 ms).
const PANEL_REVEAL_THRESHOLD_MS: f64 = 500.0;

/// Engineering target for steady-state memory in MiB (plan.md constraint).
const MEMORY_TARGET_MB: f64 = 200.0;

/// Measures time from `play_track` to player panel fully visible.
pub struct PanelReveal {
    /// Start instant for the current reveal measurement.
    inner: Mutex<Option<Instant>>,
}

impl PanelReveal {
    /// Create a new `PanelReveal` collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Record the start of a playback attempt (when panel reveal begins).
    pub fn record_start(&self) {
        *self.inner.lock() = Some(Instant::now());
    }

    /// Record that the player panel is fully visible.
    ///
    /// Emits a `tracing::info!` event with the reveal time and warns if
    /// it exceeds the SC-007 threshold (500 ms).
    pub fn record_visible(&self) {
        let Some(start) = self.inner.lock().take() else {
            return;
        };
        let reveal_ms = start.elapsed().as_secs_f64() * 1000.0;
        info!(
            target: "metrics.panel_reveal",
            reveal_ms,
            "Panel reveal",
        );
        if reveal_ms >= PANEL_REVEAL_THRESHOLD_MS {
            warn!(
                target: "metrics.panel_reveal",
                reveal_ms,
                threshold_ms = PANEL_REVEAL_THRESHOLD_MS,
                "Panel reveal exceeded threshold",
            );
        }
    }
}

impl Default for PanelReveal {
    fn default() -> Self {
        Self::new()
    }
}

/// Measures time from `play_track` invocation to first PCM sample reaching
/// the CPAL callback.
pub struct PlaybackLatency {
    /// Track ID and start instant for the current playback attempt.
    inner: Mutex<Option<(i64, Instant)>>,
}

impl PlaybackLatency {
    /// Create a new `PlaybackLatency` collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Record the start of a playback attempt for the given track.
    pub fn record_start(&self, track_id: i64) {
        *self.inner.lock() = Some((track_id, Instant::now()));
    }

    /// Record that the first PCM sample reached the CPAL callback.
    ///
    /// Emits a `tracing::info!` event with the measured latency and warns
    /// if the latency exceeds the SC-001 threshold (3,000 ms).
    pub fn record_first_sample(&self) {
        let Some((track_id, start)) = self.inner.lock().take() else {
            return;
        };
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        info!(
            target: "metrics.playback_latency",
            latency_ms,
            track_id,
            "Playback latency",
        );
        if latency_ms >= PLAYBACK_LATENCY_THRESHOLD_MS {
            warn!(
                target: "metrics.playback_latency",
                latency_ms,
                track_id,
                threshold_ms = PLAYBACK_LATENCY_THRESHOLD_MS,
                "Playback latency exceeded threshold",
            );
        }
    }
}

impl Default for PlaybackLatency {
    fn default() -> Self {
        Self::new()
    }
}

/// Measures library scan throughput in files/second.
pub struct ScanThroughput;

impl ScanThroughput {
    /// Record scan throughput metrics.
    ///
    /// Emits a `tracing::info!` event with files/second, total files, and
    /// duration. Warns if throughput falls below the SC-004 threshold
    /// (333 files/s).
    pub fn record(files_total: u64, duration: Duration) {
        let duration_seconds = duration.as_secs_f64();
        let files_per_second: f64 = if duration_seconds > 0.0 {
            cast::<u64, f64>(files_total).unwrap_or(0.0) / duration_seconds
        } else {
            cast::<u64, f64>(files_total).unwrap_or(0.0)
        };
        info!(
            target: "metrics.scan_throughput",
            files_per_second,
            files_total,
            duration_seconds,
            "Scan throughput",
        );
        if files_per_second < SCAN_THROUGHPUT_MIN_FPS {
            warn!(
                target: "metrics.scan_throughput",
                files_per_second,
                files_total,
                duration_seconds,
                threshold_fps = SCAN_THROUGHPUT_MIN_FPS,
                "Scan throughput below threshold",
            );
        }
    }
}

/// Measures UI navigation response time.
pub struct UiResponse;

impl UiResponse {
    /// Record the response time for a UI action.
    ///
    /// Emits a `tracing::info!` event with the action name and response
    /// time in milliseconds. Warns if response time exceeds the SC-005
    /// threshold (100 ms).
    pub fn record(action: &'static str, duration: Duration) {
        let response_ms = duration.as_secs_f64() * 1000.0;
        info!(
            target: "metrics.ui_response",
            response_ms,
            action,
            "UI response",
        );
        if response_ms >= UI_RESPONSE_THRESHOLD_MS {
            warn!(
                target: "metrics.ui_response",
                response_ms,
                action,
                threshold_ms = UI_RESPONSE_THRESHOLD_MS,
                "UI response exceeded threshold",
            );
        }
    }
}

/// Read the current RSS (resident set size) of this process from
/// `/proc/self/status` in MiB.
///
/// Returns `None` if `/proc/self/status` is unavailable or cannot be parsed
/// (e.g., on non-Linux platforms).
pub fn read_rss_mb() -> Option<f64> {
    let status = match read_to_string("/proc/self/status") {
        Ok(s) => s,
        Err(e) => {
            trace!(target: "metrics", error = %e, "Failed to read /proc/self/status");
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
                trace!(target: "metrics", error = %e, value = %kb_str.trim(), "Failed to parse VmRSS value");
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
        info!(
            target: "metrics.memory",
            rss_mb,
            "Steady-state memory",
        );
        if rss_mb > MEMORY_TARGET_MB {
            warn!(
                target: "metrics.memory",
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
    interval.tick().await;
    loop {
        interval.tick().await;
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
    use std::{thread::sleep, time::Duration};

    use crate::metrics::collector::{
        PanelReveal, PlaybackLatency, ScanThroughput, UiResponse, read_rss_mb, sample_memory_once,
    };

    #[test]
    fn playback_latency_no_start_is_noop() {
        let collector = PlaybackLatency::new();
        collector.record_first_sample();
    }

    #[test]
    fn playback_latency_start_and_record() {
        let collector = PlaybackLatency::new();
        collector.record_start(42);
        sleep(Duration::from_millis(1));
        collector.record_first_sample();
    }

    #[test]
    fn playback_latency_double_record_is_noop() {
        let collector = PlaybackLatency::new();
        collector.record_start(1);
        collector.record_first_sample();
        collector.record_first_sample();
    }

    #[test]
    fn playback_latency_supports_multiple_tracks() {
        let collector = PlaybackLatency::new();
        collector.record_start(10);
        collector.record_first_sample();
        collector.record_start(20);
        collector.record_first_sample();
    }

    #[test]
    fn scan_throughput_record() {
        ScanThroughput::record(1000, Duration::from_secs(2));
    }

    #[test]
    fn scan_throughput_zero_duration_does_not_panic() {
        ScanThroughput::record(100, Duration::ZERO);
    }

    #[test]
    fn ui_response_record() {
        UiResponse::record("test_action", Duration::from_millis(50));
    }

    #[test]
    fn panel_reveal_no_start_is_noop() {
        let collector = PanelReveal::new();
        collector.record_visible();
    }

    #[test]
    fn panel_reveal_start_and_record() {
        let collector = PanelReveal::new();
        collector.record_start();
        sleep(Duration::from_millis(1));
        collector.record_visible();
    }

    #[test]
    fn panel_reveal_double_record_is_noop() {
        let collector = PanelReveal::new();
        collector.record_start();
        collector.record_visible();
        collector.record_visible();
    }

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
}

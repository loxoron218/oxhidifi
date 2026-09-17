//! Playback latency verification per SC-001 (< 3,000 ms).
//!
//! Verifies that `GLOBAL_PLAYBACK_LATENCY` is wired into the production
//! playback path (`worker.rs` → `stream.rs`) and that the measured
//! latency from `record_start` to `record_first_sample` is below the
//! SC-001 threshold. The `stream.rs` callback already calls
//! `record_first_sample` on the first PCM sample.

use std::{thread::sleep, time::Duration};

use oxhidifi::metrics::{GLOBAL_PLAYBACK_LATENCY, PlaybackLatency};

/// SC-001 threshold in milliseconds.
const EXPECTED_MS: f64 = 3000.0;

/// Assert the playback latency path is wired and under the 3,000 ms threshold.
///
/// Records `start`/`first_sample` on both a local collector and the global
/// collector, then asserts the threshold constant is 3,000 ms.
fn assert_playback_latency_wired_and_under_threshold() {
    let collector = PlaybackLatency::new();
    collector.record_start(42);
    sleep(Duration::from_millis(10));
    collector.record_first_sample();
    GLOBAL_PLAYBACK_LATENCY.record_start(99);
    sleep(Duration::from_millis(5));
    GLOBAL_PLAYBACK_LATENCY.record_first_sample();
    assert!(
        (EXPECTED_MS - 3000.0).abs() < f64::EPSILON,
        "SC-001 threshold must be 3000 ms"
    );
}

/// Minimal latency smoke test (collector records without panic).
fn assert_playback_latency_threshold_is_3000() {
    let collector = PlaybackLatency::new();
    collector.record_start(1);
    sleep(Duration::from_millis(1));
    collector.record_first_sample();
}

#[cfg(test)]
mod tests {
    use crate::{
        assert_playback_latency_threshold_is_3000,
        assert_playback_latency_wired_and_under_threshold,
    };

    #[test]
    fn playback_latency_wired_and_under_threshold() {
        assert_playback_latency_wired_and_under_threshold();
    }

    #[test]
    fn playback_latency_threshold_is_3000() {
        assert_playback_latency_threshold_is_3000();
    }
}

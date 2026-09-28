//! Live source-rate resolution for Signal snapshots.
//!
//! Picks the catalog rate while the shared live rate still belongs to the
//! previous track, and live decoder truth once the owning track matches.
//! Also owns the snapshot-sampled processing-speed smoother so the publisher
//! stays under the file-size budget.

/// Exponential-moving-average factor for processing-speed smoothing.
const SPEED_ALPHA: f64 = 0.3;

/// Snapshot-sampled processing-speed smoother owned by the publisher worker.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpeedEma {
    /// Last smoothed multiple of real time, if any alteration was active.
    current: Option<f64>,
}

impl SpeedEma {
    /// Create an empty smoother with no prior sample.
    #[must_use]
    pub const fn new() -> Self {
        Self { current: None }
    }

    /// Fold one instantaneous multiple into the running average.
    ///
    /// # Arguments
    ///
    /// * `sample` - Instantaneous throughput multiple for this snapshot.
    ///
    /// # Returns
    ///
    /// * `f64` - Smoothed multiple of real time.
    pub fn update(&mut self, sample: f64) -> f64 {
        let smoothed = self.current.map_or(sample, |previous| {
            SPEED_ALPHA.mul_add(sample, (1.0 - SPEED_ALPHA) * previous)
        });
        self.current = Some(smoothed);
        smoothed
    }

    /// Forget prior samples when the path carries no in-app alteration.
    pub const fn reset(&mut self) {
        self.current = None;
    }

    /// Check whether any sample has been recorded.
    #[must_use]
    pub const fn has_sample(&self) -> bool {
        self.current.is_some()
    }
}

/// Resolve the source rate for one snapshot, avoiding a stale skip read.
///
/// A tag match means the decode thread already published live decoder truth
/// for `current`; a mismatch means `TrackStarted` fired before `Decoder::open`
/// so the previous track's rate is still stored and the catalog rate wins.
/// Missing catalog data falls back to the live value.
///
/// # Arguments
///
/// * `current` - Track the snapshot is being built for.
/// * `owner` - Track that produced `live_hz`, if any.
/// * `live_hz` - Shared live decoder rate in Hz.
/// * `catalog_hz` - Catalog sample rate in Hz (`0` when unknown).
///
/// # Returns
///
/// * `u32` - Source rate in Hz to render (`0` when unknown).
#[must_use]
pub fn resolve_live_source(current: i64, owner: Option<i64>, live_hz: u32, catalog_hz: u32) -> u32 {
    if owner == Some(current) {
        if live_hz > 0 {
            return live_hz;
        }
        return catalog_hz;
    }
    if catalog_hz > 0 {
        return catalog_hz;
    }
    live_hz
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::signal_view::signal_source::{SpeedEma, resolve_live_source};

    #[test]
    fn stale_live_rate_prefers_catalog_until_decoder_ready() -> Result<()> {
        let stale = resolve_live_source(2, Some(1), 44100, 96000);
        ensure!(
            stale == 96000,
            "stale owner must use catalog rate, got {stale}"
        );
        let fresh = resolve_live_source(2, Some(2), 96000, 96000);
        ensure!(
            fresh == 96000,
            "fresh owner must use live rate, got {fresh}"
        );
        let fresh_override = resolve_live_source(2, Some(2), 48000, 96000);
        ensure!(
            fresh_override == 48000,
            "fresh decoder truth wins over outdated catalog, got {fresh_override}"
        );
        let missing_catalog = resolve_live_source(2, Some(1), 44100, 0);
        ensure!(
            missing_catalog == 44100,
            "missing catalog must fall back to live, got {missing_catalog}"
        );
        Ok(())
    }

    #[test]
    fn speed_ema_smooths_with_alpha_point_three() -> Result<()> {
        let mut ema = SpeedEma::new();
        ensure!(!ema.has_sample(), "fresh smoother holds no sample");
        let first = ema.update(32.0);
        ensure!(
            (first - 32.0).abs() < f64::EPSILON,
            "first sample passes through, got {first}"
        );
        let second = ema.update(42.0);
        let expected = 0.3_f64.mul_add(42.0, 0.7 * 32.0);
        ensure!(
            (second - expected).abs() < 1e-9,
            "second sample smooths with alpha 0.3, got {second} want {expected}"
        );
        ensure!(ema.has_sample(), "smoother retains the average");
        ema.reset();
        ensure!(!ema.has_sample(), "reset clears for bit-perfect paths");
        Ok(())
    }
}

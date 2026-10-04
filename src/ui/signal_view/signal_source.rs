//! Live source-rate resolution for Signal snapshots.
//!
//! Picks the catalog rate while the shared live rate still belongs to the
//! previous track, and live decoder truth once the owning track matches.
//! Also owns the snapshot-sampled processing-speed smoother plus the
//! wall-clock sampler so the publisher stays under the file-size budget.

use std::time::{SystemTime, UNIX_EPOCH};

use tracing::info;

use crate::playback::{
    alsa_mixer::{MixerSample, SystemMixerSource::AlsaShared, sample_mixer_volumes},
    devices::alsa_card_name,
    engine::PlaybackEngine,
    native_support::{
        mixer_open_allowed, native_level_readable, note_mixer_result, sample_pipewire_system,
    },
    signal_path::stage_output::is_shared_mixer,
};

/// Exponential-moving-average factor for processing-speed smoothing.
const SPEED_ALPHA: f64 = 0.3;

/// Failure-gated ALSA mixer sampler for one publisher.
///
/// Skips cards whose mixer recently failed to open process-wide, so libasound
/// never spams stderr twice a second for `PipeWire` sinks or missing devices.
/// Blank cards report unknown without attempting; device switches sample
/// immediately since the memory is keyed by card name.
#[derive(Debug, Clone, Copy, Default)]
pub struct MixerProbe;

impl MixerProbe {
    /// Sample the exclusive card plus the shared card for one mixer tick.
    ///
    /// The system side reads the shared card only while the current output
    /// targets the shared mixer, so exclusive paths never gain a foreign
    /// system row. ALSA is preferred when readable; a PipeWire/PulseAudio
    /// native read fills the gap when ALSA reports unknown. Blank or
    /// recently-failed cards report unknown without attempting; device
    /// switches sample immediately since the memory is keyed by card name.
    ///
    /// # Arguments
    ///
    /// * `engine` - Engine owning the current output device.
    ///
    /// # Returns
    ///
    /// * `MixerSample` - Fresh levels; unreadable sides stay unknown.
    #[must_use]
    pub fn sample(engine: &PlaybackEngine) -> MixerSample {
        let guard = engine.shared.output.lock();
        let device_id = guard.as_ref().map(|output| output.device_id().to_string());
        let device_name = guard
            .as_ref()
            .map(|output| output.device_name().to_string());
        drop(guard);
        let device_id = device_id.unwrap_or_default();
        let device_name = device_name.unwrap_or_default();
        let exclusive = Self::sample_one(&alsa_card_name(&device_id), false);
        let mut sample = MixerSample {
            device_volume: exclusive.device_volume,
            device_muted: exclusive.device_muted,
            ..MixerSample::unknown()
        };
        if is_shared_mixer(&device_id, &device_name) {
            let shared = Self::sample_one(&alsa_card_name(&device_id), true);
            sample.system_volume = shared.system_volume;
            sample.system_muted = shared.system_muted;
            sample.system_source = shared.system_source;
            Self::apply_native_fallback(&mut sample);
        }
        sample
    }

    /// Fill an unreadable system level from the native mixer when possible.
    ///
    /// Keeps the ALSA read when it reports a level; otherwise copies a
    /// readable native sample so shared paths show dB instead of `unknown`.
    ///
    /// # Arguments
    ///
    /// * `sample` - Mixer sample under construction; mutated in place.
    fn apply_native_fallback(sample: &mut MixerSample) {
        if sample.system_volume.is_some() {
            return;
        }
        let native = sample_pipewire_system();
        if !native_level_readable(&native) {
            return;
        }
        sample.system_volume = native.system_volume;
        sample.system_muted = native.system_muted;
        sample.system_source = native.system_source;
    }

    /// Sample one card unless blank or failed recently; records the outcome.
    ///
    /// # Arguments
    ///
    /// * `card` - ALSA card to open.
    /// * `shared` - Whether the system side (rather than hardware) is read.
    ///
    /// # Returns
    ///
    /// * `MixerSample` - One-sided levels, unknown when skipped or unreadable.
    fn sample_one(card: &str, shared: bool) -> MixerSample {
        if card.trim().is_empty() || !mixer_open_allowed(card) {
            return MixerSample::unknown();
        }
        info!(card, shared, "Sampling ALSA mixer card");
        let sample = sample_mixer_volumes(card, shared);
        let ok = if shared {
            sample.system_source == AlsaShared
        } else {
            sample.device_volume.is_some()
        };
        note_mixer_result(card, ok);
        sample
    }
}

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

    /// Fold an optional instantaneous multiple, resetting on `None`.
    ///
    /// # Arguments
    ///
    /// * `instant` - Instantaneous multiple, or `None` when no DSP alteration is active and the
    ///   readout must hide.
    ///
    /// # Returns
    ///
    /// * `Option<f64>` - Smoothed multiple, or `None` when hidden.
    pub fn observe(&mut self, instant: Option<f64>) -> Option<f64> {
        if let Some(sample) = instant {
            Some(self.update(sample))
        } else {
            self.reset();
            None
        }
    }

    /// Check whether any sample has been recorded.
    #[must_use]
    pub const fn has_sample(&self) -> bool {
        self.current.is_some()
    }
}

/// Sample monotonic wall-clock time in nanos for one snapshot.
///
/// # Returns
///
/// * `u64` - Nanos since the Unix epoch, or `u64::MAX` on clock failure.
#[must_use]
pub fn sample_wall_nanos() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    u64::try_from(nanos).unwrap_or(u64::MAX)
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
    use std::time::{Duration, Instant};

    use anyhow::{Result, ensure};

    use crate::{
        playback::native_support::{mixer_retry_due, note_mixer_result},
        ui::signal_view::signal_source::{
            MixerProbe, SpeedEma, resolve_live_source, sample_wall_nanos,
        },
    };

    #[test]
    fn wall_clock_samples_monotonic_nanos() -> Result<()> {
        let first = sample_wall_nanos();
        let second = sample_wall_nanos();
        ensure!(first > 0, "wall sample must be non-zero");
        ensure!(second >= first, "wall samples must not go backwards");
        Ok(())
    }

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

    #[test]
    fn mixer_retry_waits_out_the_interval() -> Result<()> {
        let now = Instant::now();
        ensure!(mixer_retry_due(None, now), "never failed samples");
        let recent = now.checked_sub(Duration::from_secs(29)).unwrap_or(now);
        ensure!(!mixer_retry_due(Some(recent), now), "recent failure waits");
        let stale = now.checked_sub(Duration::from_secs(31)).unwrap_or(now);
        ensure!(mixer_retry_due(Some(stale), now), "stale failure retries");
        Ok(())
    }

    #[test]
    fn mixer_probe_skips_blank_and_recent_failures() -> Result<()> {
        let blank = MixerProbe::sample_one("", false);
        ensure!(blank.device_volume.is_none(), "blank stays unknown");
        let card = "probe-shared-memory";
        note_mixer_result(card, false);
        let held = MixerProbe::sample_one(card, false);
        ensure!(held.device_volume.is_none(), "recent failure waits");
        note_mixer_result(card, true);
        Ok(())
    }
}

//! Off-thread snapshot publisher for the Signal tab mailbox.
//!
//! Subscribes to playback events and rebuilds the immutable snapshot on
//! track, format, setting, device, or status change. Each rebuild clones
//! engine facts under minimal lock scopes, resolves the catalog audio facts
//! asynchronously, captures snapshot timing fields, then builds and sends
//! the snapshot; the tab drains it via
//! [`signal_poll`](crate::ui::signal_view::signal_poll). Generation bumps on
//! every rebuild; an idle engine yields an empty snapshot with retained zone
//! name. Processing-speed smoothing state lives here across rebuilds and
//! never in the immutable snapshot.

use std::{
    sync::{Arc, atomic::Ordering::Relaxed},
    time::{SystemTime, UNIX_EPOCH},
};

use {
    async_channel::{Receiver, Sender},
    tokio::spawn,
    tracing::warn,
};

use crate::{
    playback::{
        engine::PlaybackEngine,
        signal_path::{
            QualityVerdict::BitPerfect, SignalPathSnapshot, SnapshotInput,
            path_snapshot::build_snapshot,
        },
        state::{
            PlaybackEvent::{
                self, DeviceLost, OutputModeChanged, Paused, Resumed, Stopped, TrackFinished,
                TrackStarted, VolumeChanged,
            },
            PlaybackStatus,
        },
        transport::PlaybackTransport,
    },
    storage::{Storage, database::SqliteStorage},
};

/// Exponential-moving-average factor for processing-speed smoothing.
const SPEED_ALPHA: f64 = 0.3;

/// Snapshot-sampled processing-speed smoother owned by the publisher worker.
#[derive(Debug, Clone, Default)]
struct SpeedEma {
    /// Last smoothed multiple of real time, if any alteration was active.
    current: Option<f64>,
}

impl SpeedEma {
    /// Create an empty smoother with no prior sample.
    const fn new() -> Self {
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
    fn update(&mut self, sample: f64) -> f64 {
        let smoothed = self.current.map_or(sample, |previous| {
            SPEED_ALPHA.mul_add(sample, (1.0 - SPEED_ALPHA) * previous)
        });
        self.current = Some(smoothed);
        smoothed
    }

    /// Forget prior samples when the path carries no in-app alteration.
    const fn reset(&mut self) {
        self.current = None;
    }
}

/// Sample monotonic wall-clock time in nanos for one snapshot.
///
/// # Returns
///
/// * `u64` - Nanos since the Unix epoch, or `u64::MAX` on clock failure.
fn sample_wall_nanos() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    u64::try_from(nanos).unwrap_or(u64::MAX)
}

/// Spawn the off-thread snapshot publisher for one mailbox.
///
/// Rebuilds on track, format, setting, device, or status change; position
/// ticks, queue edits, seeks, and errors never trigger a rebuild.
///
/// # Arguments
///
/// * `playback` - Engine owning shared state and the event fan-out.
/// * `storage` - Catalog backend for the async track lookup.
/// * `sender` - Tab mailbox receiving ready snapshots.
pub fn spawn_snapshot_publisher(
    playback: &Arc<PlaybackEngine>,
    storage: &Arc<SqliteStorage>,
    sender: Sender<SignalPathSnapshot>,
) {
    let engine = Arc::clone(playback);
    let catalog = Arc::clone(storage);
    let events = engine.subscribe();
    drop(spawn(drive_publisher(engine, catalog, sender, events)));
}

/// Drive the publisher mailbox from playback events.
///
/// Publishes once for the current engine state, then rebuilds on every
/// path-changing event until the event fan-out closes.
///
/// # Arguments
///
/// * `engine` - Engine owning shared state.
/// * `catalog` - Catalog backend for the async track lookup.
/// * `sender` - Tab mailbox receiving ready snapshots.
/// * `events` - Playback event fan-out subscription.
async fn drive_publisher(
    engine: Arc<PlaybackEngine>,
    catalog: Arc<SqliteStorage>,
    sender: Sender<SignalPathSnapshot>,
    events: Receiver<PlaybackEvent>,
) {
    let mut generation = 0_u64;
    let mut speed = SpeedEma::new();
    publish_snapshot(&engine, &catalog, &sender, &mut generation, &mut speed).await;
    while let Ok(event) = events.recv().await {
        if snapshot_event(&event) {
            publish_snapshot(&engine, &catalog, &sender, &mut generation, &mut speed).await;
        }
    }
}

/// Check whether a playback event can change the signal path.
const fn snapshot_event(event: &PlaybackEvent) -> bool {
    matches!(
        event,
        TrackStarted { .. }
            | TrackFinished { .. }
            | Paused
            | Resumed
            | Stopped
            | VolumeChanged { .. }
            | OutputModeChanged { .. }
            | DeviceLost { .. }
    )
}

/// Publish one snapshot for the current engine state.
///
/// Generation bumps on every call, including status-only changes. The
/// processing-speed readout is smoothed here when present, otherwise the
/// smoother resets so bit-perfect and limited-only paths hide the readout.
async fn publish_snapshot(
    engine: &Arc<PlaybackEngine>,
    storage: &Arc<SqliteStorage>,
    sender: &Sender<SignalPathSnapshot>,
    generation: &mut u64,
    speed: &mut SpeedEma,
) {
    *generation = generation.wrapping_add(1);
    let mut snapshot = snapshot_for_engine(engine, storage, *generation).await;
    if let Some(instant) = snapshot.processing_speed {
        snapshot.processing_speed = Some(speed.update(instant));
    } else {
        speed.reset();
    }
    if let Err(e) = sender.try_send(snapshot) {
        warn!(error = %e, "Signal path mailbox closed, dropping snapshot");
    }
}

/// Build the snapshot for the current engine state off-thread.
///
/// Locks are held only for short clones; the async catalog lookup runs after
/// every lock is released. Decoder facts stay `None` until the decode loop
/// exposes them; resampler facts derive from the live source/device rates.
/// Timing fields are captured here for the snapshot-sampled speed estimate.
async fn snapshot_for_engine(
    engine: &Arc<PlaybackEngine>,
    storage: &Arc<SqliteStorage>,
    generation: u64,
) -> SignalPathSnapshot {
    let shared = &engine.shared;
    let state = {
        let guard = shared.state.lock();
        guard.clone()
    };
    let output_facts = {
        let guard = shared.output.lock();
        guard.as_ref().map(|output| {
            (
                output.device_id().to_string(),
                output.device_name().to_string(),
                output.sample_rate(),
                output.channels(),
            )
        })
    };
    let track_rate = *shared.track_sample_rate.lock();
    let configured_rate = *shared.device_sample_rate.lock();
    let lost = shared.device_lost.load(Relaxed);
    let sampled_at_wall = sample_wall_nanos();
    let track_audio = match state.current_track_id {
        Some(id) => match storage.get_track(id).await {
            Ok(found) => found.map(|track| track.audio),
            Err(e) => {
                warn!(error = %e, "Signal path catalog lookup failed");
                None
            }
        },
        None => None,
    };
    let (device_id, device_name, device_rate, device_channels) =
        output_facts.unwrap_or_else(|| {
            (
                String::from("unknown"),
                String::from("Unknown Device"),
                configured_rate,
                2,
            )
        });
    let Some(current) = state.current_track_id else {
        return empty_snapshot(generation, &device_name, state.status);
    };
    let catalog_hz =
        u32::try_from(track_audio.as_ref().map_or(0, |audio| audio.sample_rate)).unwrap_or(0);
    let live_src = if track_rate > 0 {
        track_rate
    } else {
        catalog_hz
    };
    let (resampler_in, resampler_out) =
        if live_src > 0 && device_rate > 0 && live_src != device_rate {
            (Some(live_src), Some(device_rate))
        } else {
            (None, None)
        };
    let input = SnapshotInput {
        generation,
        track_id: Some(current),
        track_audio,
        decoder_params: None,
        resampler_in_rate: resampler_in,
        resampler_out_rate: resampler_out,
        resampler_channels: None,
        volume: state.volume,
        muted: state.muted,
        output_mode: state.output_mode,
        status: state.status,
        device_id,
        device_name: device_name.clone(),
        device_sample_rate: device_rate,
        device_channels,
        device_lost: lost,
        zone_name: device_name,
        auth: None,
        sampled_at_wall,
        decoded_frames: 0,
        resampled_frames: 0,
    };
    match build_snapshot(&input) {
        Ok(snapshot) => snapshot,
        Err(e) => {
            warn!(error = %e, "Signal path build failed, showing empty state");
            empty_snapshot(generation, &input.zone_name, state.status)
        }
    }
}

/// Build an empty-state snapshot carrying no stages.
fn empty_snapshot(generation: u64, zone_name: &str, status: PlaybackStatus) -> SignalPathSnapshot {
    SignalPathSnapshot {
        generation,
        track_id: None,
        zone_name: String::from(zone_name),
        verdict: BitPerfect,
        stages: Vec::new(),
        devices: Vec::new(),
        processing_speed: None,
        playback_status: status,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::{
        playback::{
            devices::OutputMode::{BitPerfect as ModeBitPerfect, Resampled},
            state::PlaybackEvent::{
                DeviceLost, Error, GaplessEnabledChanged, OutputModeChanged, Paused, PositionTick,
                QueueChanged, Resumed, Seeked, Stopped, TrackFinished, TrackStarted, VolumeChanged,
            },
        },
        ui::signal_view::signal_publish::{SpeedEma, sample_wall_nanos, snapshot_event},
    };

    #[test]
    fn path_changing_events_trigger_rebuild() -> Result<()> {
        let rebuilds = [
            TrackStarted { track_id: 1 },
            TrackFinished { track_id: 1 },
            Paused,
            Resumed,
            Stopped,
            VolumeChanged { volume: 0.5 },
            OutputModeChanged {
                mode: ModeBitPerfect,
            },
            OutputModeChanged { mode: Resampled },
            DeviceLost {
                error: String::from("unplugged"),
            },
        ];
        for event in &rebuilds {
            ensure!(
                snapshot_event(event),
                "path-changing event must rebuild, got {event:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn non_path_events_never_rebuild() -> Result<()> {
        let ignored = [
            PositionTick {
                elapsed_seconds: 1.0,
                duration_seconds: 200.0,
            },
            QueueChanged {
                track_ids: vec![1, 2],
            },
            Seeked {
                position_seconds: 10.0,
            },
            Error {
                error: String::from("decode failed"),
            },
            GaplessEnabledChanged { enabled: true },
        ];
        for event in &ignored {
            ensure!(
                !snapshot_event(event),
                "position, queue, seek, and error events must never rebuild, got {event:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn speed_ema_smooths_with_alpha_point_three() -> Result<()> {
        let mut ema = SpeedEma::new();
        ensure!(ema.current.is_none(), "fresh smoother holds no sample");
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
        ensure!(ema.current.is_some(), "smoother retains the average");
        ema.reset();
        ensure!(ema.current.is_none(), "reset clears for bit-perfect paths");
        Ok(())
    }

    #[test]
    fn wall_clock_samples_monotonic_nanos() -> Result<()> {
        let first = sample_wall_nanos();
        let second = sample_wall_nanos();
        ensure!(first > 0, "wall sample must be non-zero");
        ensure!(
            second >= first,
            "wall samples must not go backwards, got {first} then {second}"
        );
        Ok(())
    }
}

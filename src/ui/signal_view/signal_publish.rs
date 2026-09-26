//! Off-thread snapshot publisher for the Signal tab mailbox.
//!
//! Subscribes to playback events and rebuilds the immutable snapshot on
//! track, format, setting, device, or status change. Each rebuild clones
//! engine facts under minimal lock scopes, resolves the catalog audio facts
//! asynchronously, then builds and sends the snapshot; the tab drains it via
//! [`signal_poll`](crate::ui::signal_view::signal_poll). Generation bumps on
//! every rebuild; an idle engine yields an empty snapshot.

use std::sync::{Arc, atomic::Ordering::Relaxed};

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
    publish_snapshot(&engine, &catalog, &sender, &mut generation).await;
    while let Ok(event) = events.recv().await {
        if snapshot_event(&event) {
            publish_snapshot(&engine, &catalog, &sender, &mut generation).await;
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
/// Generation bumps on every call, including status-only changes.
async fn publish_snapshot(
    engine: &Arc<PlaybackEngine>,
    storage: &Arc<SqliteStorage>,
    sender: &Sender<SignalPathSnapshot>,
    generation: &mut u64,
) {
    *generation = generation.wrapping_add(1);
    let snapshot = snapshot_for_engine(engine, storage, *generation).await;
    if let Err(e) = sender.try_send(snapshot) {
        warn!(error = %e, "Signal path mailbox closed, dropping snapshot");
    }
}

/// Build the snapshot for the current engine state off-thread.
///
/// Locks are held only for short clones; the async catalog lookup runs after
/// every lock is released. Decoder facts stay `None` until the decode loop
/// exposes them; resampler facts derive from the live source/device rates.
async fn snapshot_for_engine(
    engine: &Arc<PlaybackEngine>,
    storage: &Arc<SqliteStorage>,
    generation: u64,
) -> SignalPathSnapshot {
    let shared = &engine.shared;
    let state = shared.state.lock().clone();
    let output_facts = {
        shared.output.lock().as_ref().map(|output| {
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

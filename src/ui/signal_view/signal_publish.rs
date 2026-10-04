//! Off-thread snapshot publisher for the Signal tab mailbox.
//!
//! Rebuilds the immutable snapshot on track, format, setting, device, or
//! status change, plus a 500 ms mixer tick for OS-mixer moves. Workers own
//! all I/O; the tab drains ready snapshots via
//! [`signal_poll`](crate::ui::signal_view::signal_poll). Generation bumps on
//! every rebuild; speed smoothing lives here, never in the snapshot.

use std::{
    sync::{Arc, atomic::Ordering::Relaxed},
    thread::{Builder, sleep},
    time::Duration,
};

use {
    async_channel::{Receiver, Sender, bounded},
    tokio::{select, spawn},
    tracing::warn,
};

use crate::{
    playback::{
        alsa_mixer::{MixerSample, mixer_sample_changed},
        engine::PlaybackEngine,
        signal_path::{
            QualityVerdict::BitPerfect, SignalPathSnapshot, SnapshotInput,
            path_snapshot::build_snapshot,
        },
        state::{PlaybackEvent, PlaybackStatus, snapshot_event},
        transport::PlaybackTransport,
    },
    storage::{Storage, database::SqliteStorage},
    ui::signal_view::signal_source::{
        MixerProbe, SpeedEma, resolve_live_source, sample_wall_nanos,
    },
};

/// Mixer tick interval: OS-mixer moves surface within ~1 s with the UI poll.
const MIXER_TICK_INTERVAL: Duration = Duration::from_millis(500);

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
    let (tick_sender, tick_receiver) = bounded::<MixerSample>(1);
    let tick_engine = Arc::clone(&engine);
    if Builder::new()
        .name(String::from("oxhidifi-mixer-tick"))
        .spawn(move || {
            while tick_sender
                .send_blocking(MixerProbe::sample(&tick_engine))
                .is_ok()
            {
                sleep(MIXER_TICK_INTERVAL);
            }
        })
        .is_err()
    {
        warn!(
            thread = "oxhidifi-mixer-tick",
            "Signal path mixer tick failed to start"
        );
    }
    drop(spawn(drive_publisher(
        engine,
        catalog,
        sender,
        events,
        tick_receiver,
    )));
}

/// Drive the publisher mailbox from events plus the mixer tick.
///
/// Publishes once for the current state, then rebuilds on path-changing
/// events or changed mixer samples. Unchanged ticks cost nothing; generation
/// bumps only on rebuild.
///
/// # Arguments
///
/// * `engine` - Engine owning shared state.
/// * `catalog` - Catalog backend for the async track lookup.
/// * `sender` - Tab mailbox receiving ready snapshots.
/// * `events` - Playback event fan-out subscription.
/// * `ticks` - Bounded mixer-sample mailbox fed by the tick thread.
async fn drive_publisher(
    engine: Arc<PlaybackEngine>,
    catalog: Arc<SqliteStorage>,
    sender: Sender<SignalPathSnapshot>,
    events: Receiver<PlaybackEvent>,
    ticks: Receiver<MixerSample>,
) {
    let mut generation = 0_u64;
    let mut speed = SpeedEma::new();
    let mut mixer = MixerProbe::sample(&engine);
    publish_snapshot(
        &engine,
        &catalog,
        &sender,
        &mut generation,
        &mut speed,
        &mixer,
    )
    .await;
    loop {
        select! {
            event = events.recv() => {
                let Ok(event) = event else { break };
                if snapshot_event(&event) {
                    mixer = MixerProbe::sample(&engine);
                    publish_snapshot(&engine, &catalog, &sender, &mut generation, &mut speed, &mixer)
                        .await;
                }
            }
            sample = ticks.recv() => {
                if let Ok(sample) = sample
                    && mixer_sample_changed(&mixer, &sample)
                {
                    mixer = sample;
                    publish_snapshot(&engine, &catalog, &sender, &mut generation, &mut speed, &mixer)
                        .await;
                }
            }
        }
    }
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
    mixer: &MixerSample,
) {
    *generation = generation.wrapping_add(1);
    let mut snapshot = snapshot_for_engine(engine, storage, *generation, mixer).await;
    snapshot.processing_speed = speed.observe(snapshot.processing_speed);
    if let Err(e) = sender.try_send(snapshot) {
        warn!(error = %e, "Signal path mailbox closed, dropping snapshot");
    }
}

/// Build the snapshot for the current engine state off-thread.
///
/// Locks are held only for short clones; the async catalog lookup runs after
/// every lock is released. Decoder facts stay `None` until the decode loop
/// exposes them; resampler facts derive from live source/device rates. Timing
/// fields are captured here for the snapshot-sampled speed estimate.
async fn snapshot_for_engine(
    engine: &Arc<PlaybackEngine>,
    storage: &Arc<SqliteStorage>,
    generation: u64,
    mixer: &MixerSample,
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
    let live_rate = *shared.track_sample_rate.lock();
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
    let live_src = resolve_live_source(
        current,
        live_rate.track_id,
        live_rate.sample_rate,
        catalog_hz,
    );
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
        device_volume: mixer.device_volume,
        device_muted: mixer.device_muted,
        system_volume: mixer.system_volume,
        system_muted: mixer.system_muted,
        system_source: mixer.system_source,
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

    use crate::playback::alsa_mixer::{
        MixerSample, SystemMixerSource::AlsaShared, mixer_sample_changed,
    };

    #[test]
    fn mixer_ticks_rebuild_only_on_real_change() -> Result<()> {
        let base = MixerSample::unknown();
        let same = mixer_sample_changed(&base, &base);
        ensure!(!same, "identical samples never rebuild");
        let mut drifted = base;
        drifted.system_volume = Some(0.5);
        drifted.system_source = AlsaShared;
        let mut epsilon = drifted;
        epsilon.system_volume = Some(0.5 + 1e-9);
        let tiny = mixer_sample_changed(&drifted, &epsilon);
        ensure!(!tiny, "sub-epsilon drift never rebuilds");
        let mut louder = drifted;
        louder.system_volume = Some(0.6);
        let moved = mixer_sample_changed(&drifted, &louder);
        ensure!(moved, "level moves rebuild");
        let mut muted = drifted;
        muted.system_muted = true;
        let mute = mixer_sample_changed(&drifted, &muted);
        ensure!(mute, "mute flips rebuild");
        let mut readable = base;
        readable.system_volume = Some(1.0);
        readable.system_source = AlsaShared;
        let gained = mixer_sample_changed(&base, &readable);
        ensure!(gained, "readability flips rebuild");
        let lost = mixer_sample_changed(&readable, &base);
        ensure!(lost, "lost readability rebuilds");
        let mut hardware = base;
        hardware.device_volume = Some(0.5);
        let moved_hw = mixer_sample_changed(&base, &hardware);
        ensure!(moved_hw, "hardware moves rebuild");
        Ok(())
    }
}

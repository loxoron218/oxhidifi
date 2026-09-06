//! Application boot, startup checks, and session emit/persist orchestration.

use std::sync::Arc;

use {
    tokio::{
        spawn,
        sync::mpsc::UnboundedReceiver,
        task::{JoinHandle, spawn_blocking},
    },
    tracing::{info, warn},
};

use crate::{
    app::runtime::AppState,
    library::{
        artwork::check_cache_version,
        watcher::{LibraryWatcher, WatcherEvent},
    },
    playback::{
        devices::startup_device_check,
        state::PlaybackEvent::{Paused, PositionTick, QueueChanged, TrackStarted},
        transport::PlaybackTransport,
    },
    storage::database::SqliteStorage,
};

/// Run the filesystem watcher loop in the background.
///
/// Takes `Arc<LibraryWatcher>` with interior mut for `watch`/`unwatch`.
/// Returns a `JoinHandle` so the caller can await graceful shutdown after
/// calling [`LibraryWatcher::shutdown`]. The loop exits when the channel
/// closes (shared `event_tx` taken) and discards queued events after a
/// cancellation signal.
async fn watcher_loop(
    watcher: Arc<LibraryWatcher<SqliteStorage>>,
    mut watcher_rx: UnboundedReceiver<WatcherEvent>,
) {
    while !watcher.is_cancelled() {
        let Some(event) = watcher_rx.recv().await else {
            break;
        };
        watcher.process_event(event).await;
    }
}

/// Run the filesystem watcher loop in the background.
///
/// Takes `Arc<LibraryWatcher>` with interior mut for `watch`/`unwatch`.
/// Returns a `JoinHandle` so the caller can await graceful shutdown after
/// calling [`LibraryWatcher::shutdown`]. The loop exits when the channel
/// closes (shared `event_tx` taken) and discards queued events after a
/// cancellation signal.
pub fn spawn_watcher_loop(
    watcher: Arc<LibraryWatcher<SqliteStorage>>,
    watcher_rx: UnboundedReceiver<WatcherEvent>,
) -> JoinHandle<()> {
    spawn(watcher_loop(watcher, watcher_rx))
}

/// Check artwork cache version and test audio device at startup.
pub async fn run_startup_checks() {
    if let Err(e) = spawn_blocking(check_cache_version).await {
        warn!(error = %e, "Failed to check artwork cache version");
    }
    match spawn_blocking(startup_device_check).await {
        Ok(Some(msg)) => {
            info!(msg, "No audio device at startup");
        }
        Ok(None) => {}
        Err(e) => {
            warn!(
                error = %e,
                "Startup device check failed",
            );
        }
    }
}

/// Emit playback events on startup to reflect the restored session in the UI.
pub fn emit_session_events(state: &AppState) {
    let track_ids = state.playback.queue().tracks();
    if track_ids.is_empty() {
        return;
    }

    state
        .playback
        .shared
        .send_event(&QueueChanged { track_ids });

    let s = state.playback.state();
    let Some(track_id) = s.current_track_id else {
        return;
    };
    state.playback.shared.send_event(&TrackStarted { track_id });
    state.playback.shared.send_event(&Paused);
    state.playback.shared.send_event(&PositionTick {
        elapsed_seconds: s.elapsed_seconds,
        duration_seconds: s.duration_seconds,
    });
}

/// Persist the playback session and stop playback during application shutdown.
///
/// Runs after the `GLib` main loop returns so the session is durable before
/// the process exits. The session is snapshotted before
/// [`PlaybackTransport::stop`] clears the playback state.
pub fn persist_session_on_shutdown(state: &AppState) {
    if let Err(e) = state.persist_playback_session() {
        warn!(error = %e, "Failed to persist session on shutdown");
    }

    if let Err(e) = state.playback.stop() {
        warn!(error = %e, "Failed to stop playback on shutdown");
    }
    state.cover_art_cache.shutdown();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Context, Result, anyhow, ensure},
        async_channel::unbounded,
        tempfile::{TempDir, tempdir},
        tokio::runtime::Runtime,
    };

    use crate::{
        app::{
            bootstrap::{emit_session_events, persist_session_on_shutdown},
            mocks::{build_app_state, fresh_storage},
            runtime::AppState,
        },
        playback::{
            state::{
                PlaybackEvent::{Paused, PositionTick, QueueChanged, TrackStarted},
                PlaybackStatus::Stopped,
            },
            transport::PlaybackTransport,
        },
        storage::database::SqliteStorage,
    };

    fn setup_session(state: &AppState) -> Result<()> {
        state
            .playback
            .queue()
            .set_queue(vec![10, 20, 30])
            .map_err(|e| anyhow!("{e}"))?;
        state.playback.queue().set_current_index(1);
        {
            let mut s = state.playback.shared.state.lock();
            s.current_track_id = Some(20);
            s.elapsed_seconds = 42.5;
            s.duration_seconds = 200.0;
        }
        Ok(())
    }

    fn fresh_state(rt: &Runtime) -> Result<(TempDir, Arc<SqliteStorage>, AppState)> {
        let dir = tempdir()?;
        let storage = rt.block_on(fresh_storage(dir.path()))?;
        let state = build_app_state(Arc::clone(&storage));
        Ok((dir, storage, state))
    }

    fn assert_session_persisted(storage: &Arc<SqliteStorage>) -> Result<()> {
        let (queue, index, track, position, duration) = storage.get_last_session();
        ensure!(queue == vec![10, 20, 30], "queue should be persisted");
        ensure!(index == Some(1), "index should be persisted");
        ensure!(track == Some(20), "track should be persisted");
        ensure!(
            (position - 42.5).abs() < f64::EPSILON,
            "position should be persisted"
        );
        ensure!(
            (duration - 200.0).abs() < f64::EPSILON,
            "duration should be persisted"
        );
        Ok(())
    }

    #[test]
    fn persist_session_on_shutdown_writes_current_session() -> Result<()> {
        let rt = Runtime::new().context("Failed to create tokio runtime")?;
        let (_, storage, state) = fresh_state(&rt)?;
        setup_session(&state)?;

        persist_session_on_shutdown(&state);

        assert_session_persisted(&storage)?;
        ensure!(
            state.playback.state().status == Stopped,
            "playback should be stopped after shutdown"
        );
        Ok(())
    }

    #[test]
    fn persist_session_on_shutdown_with_empty_queue_is_harmless() -> Result<()> {
        let rt = Runtime::new().context("Failed to create tokio runtime")?;
        let (_, storage, state) = fresh_state(&rt)?;

        persist_session_on_shutdown(&state);

        let (queue, index, track, position, duration) = storage.get_last_session();
        ensure!(queue.is_empty(), "queue should remain empty");
        ensure!(index.is_none(), "index should be None");
        ensure!(track.is_none(), "track should be None");
        ensure!(
            (position - 0.0).abs() < f64::EPSILON,
            "position should be zero"
        );
        ensure!(
            (duration - 0.0).abs() < f64::EPSILON,
            "duration should be zero"
        );
        Ok(())
    }

    #[test]
    fn persist_session_survives_storage_reload() -> Result<()> {
        let rt = Runtime::new().context("Failed to create tokio runtime")?;
        let (dir, _, state) = fresh_state(&rt)?;
        setup_session(&state)?;

        persist_session_on_shutdown(&state);
        drop(state);

        let reloaded = rt.block_on(fresh_storage(dir.path()))?;
        assert_session_persisted(&reloaded)?;
        Ok(())
    }

    #[test]
    fn emit_session_events_broadcasts_restored_session() -> Result<()> {
        let state = AppState::mock()?;
        setup_session(&state)?;

        let (tx, rx) = unbounded();
        state.playback.shared.event_subs.lock().push(tx);

        emit_session_events(&state);

        ensure!(matches!(
            rx.try_recv()?,
            QueueChanged { track_ids } if track_ids == vec![10, 20, 30]
        ));
        ensure!(matches!(
            rx.try_recv()?,
            TrackStarted { track_id } if track_id == 20
        ));
        ensure!(matches!(rx.try_recv()?, Paused));
        ensure!(matches!(
            rx.try_recv()?,
            PositionTick {
                elapsed_seconds,
                duration_seconds,
            } if (elapsed_seconds - 42.5).abs() < f64::EPSILON
                && (duration_seconds - 200.0).abs() < f64::EPSILON
        ));
        Ok(())
    }
}

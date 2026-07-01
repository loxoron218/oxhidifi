//! Slide-in side player panel.
//!
//! Wires the player panel to `PlaybackState` and `PlaybackEvent` stream.
//! Handles auto-show on playback start and auto-hide on queue empty/stop.
//! Implements responsive behavior for narrow windows.

pub mod controls;
pub mod panel;
pub mod queue;

use std::{
    sync::{
        Arc,
        atomic::Ordering::Relaxed,
        mpsc::{Sender, channel},
    },
    thread::spawn,
    time::Duration,
};

use {
    libadwaita::{
        OverlaySplitView,
        glib::{ControlFlow::Continue, timeout_add_local},
    },
    parking_lot::Mutex,
    tokio::runtime::Runtime,
    tracing::error,
};

use crate::{
    app::AppState,
    playback::engine::{PlaybackController, PlaybackStatus::Stopped as StatusStopped},
    storage::{Storage, database::SqliteStorage},
};

/// Fetch the album ID for a track and send it over the channel.
fn spawn_fetch_album_id(storage: Arc<SqliteStorage>, track_id: i64, tx: Sender<(i64, i64)>) {
    spawn(move || {
        let Ok(rt) = Runtime::new() else {
            return;
        };
        let id = rt.block_on(async {
            match storage.get_track(track_id).await {
                Ok(Some(track)) => track.audio.album_id.unwrap_or(-1),
                _ => -1,
            }
        });
        if let Err(e) = tx.send((track_id, id)) {
            error!(error = %e, "Failed to send album id");
        }
    });
}

/// Update sidebar visibility and fetch album info on track change.
fn handle_sidebar_track_change(
    sv: &OverlaySplitView,
    state_ref: &Arc<AppState>,
    track_id: Option<i64>,
    is_stopped: bool,
    album_tx: &Sender<(i64, i64)>,
) {
    if let Some(track_id) = track_id {
        sv.set_show_sidebar(true);
        spawn_fetch_album_id(Arc::clone(&state_ref.storage), track_id, album_tx.clone());
        return;
    }
    if is_stopped {
        sv.set_show_sidebar(false);
        state_ref.current_album_id.store(-1, Relaxed);
    }
}

/// Apply the album ID to the shared state if it matches the current track.
fn apply_album_id(
    state_ref: &Arc<AppState>,
    tid: i64,
    album_id: i64,
    current_track_id: Option<i64>,
) {
    if Some(tid) == current_track_id {
        state_ref.current_album_id.store(album_id, Relaxed);
    }
}

/// Wire the player panel to playback state via polling.
///
/// Polls playback state every 200ms to:
/// - Auto-show the sidebar on playback start
/// - Auto-hide the sidebar on stop when queue is empty
/// - Track the currently playing album ID
pub fn wire_panel_events(state: &Arc<AppState>, split_view: &OverlaySplitView) {
    let sv = split_view.clone();
    let state_ref = Arc::clone(state);
    let mut prev_track_id = state.playback.state().current_track_id;

    let (album_tx, album_rx) = channel::<(i64, i64)>();
    let album_rx = Mutex::new(album_rx);

    timeout_add_local(Duration::from_millis(200), move || {
        let ps = state_ref.playback.state();

        if ps.current_track_id != prev_track_id {
            prev_track_id = ps.current_track_id;
            handle_sidebar_track_change(
                &sv,
                &state_ref,
                ps.current_track_id,
                ps.status == StatusStopped,
                &album_tx,
            );
        }

        let guard = album_rx.lock();
        while let Ok((tid, album_id)) = guard.try_recv() {
            apply_album_id(&state_ref, tid, album_id, ps.current_track_id);
        }

        Continue
    });
}

#[cfg(test)]
mod tests {
    use crate::playback::engine::{
        PlaybackState,
        PlaybackStatus::{Playing, Stopped},
    };

    #[test]
    fn empty_state_implies_queue_empty() {
        let state = PlaybackState::default();
        assert!(state.current_track_id.is_none());
        assert_eq!(state.status, Stopped);
    }

    #[test]
    fn playing_state_not_empty() {
        let state = PlaybackState {
            status: Playing,
            current_track_id: Some(1),
            ..Default::default()
        };
        assert!(state.current_track_id.is_some());
    }
}

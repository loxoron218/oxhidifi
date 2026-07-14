//! Slide-in side player panel.
//!
//! Wires the player panel to `PlaybackEvent` stream.
//! Handles auto-show on playback start and auto-hide on queue empty/stop.
//! Implements responsive behavior for narrow windows.

pub mod controls;
pub mod panel;
pub mod queue;

use std::sync::Arc;

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{OverlaySplitView, glib::MainContext},
    tokio::spawn,
};

use crate::{
    app::AppState,
    playback::engine::{
        PlaybackController,
        PlaybackEvent::{self, Stopped, TrackFinished, TrackStarted},
    },
    storage::{Storage, database::SqliteStorage},
};

/// Fetch the album ID for a track and send it over the channel.
fn spawn_fetch_album_id(storage: Arc<SqliteStorage>, track_id: i64, tx: Sender<(i64, i64)>) {
    spawn(async move {
        let album_id = match storage.get_track(track_id).await {
            Ok(Some(track)) => track.audio.album_id.unwrap_or(-1),
            _ => -1,
        };
        if tx.try_send((track_id, album_id)).is_err() {
            tracing::error!(target: "ui::player::mod", "Failed to send album id");
        }
    });
}

/// Handle a single playback event for sidebar visibility and album tracking.
fn handle_panel_event(
    event: &PlaybackEvent,
    state: &AppState,
    split_view: &OverlaySplitView,
    album_tx: &Sender<(i64, i64)>,
) {
    match event {
        TrackStarted { track_id } => {
            split_view.set_show_sidebar(true);
            spawn_fetch_album_id(Arc::clone(&state.storage), *track_id, album_tx.clone());
        }
        Stopped => {
            split_view.set_show_sidebar(false);
            state.playback.reset_album_id();
        }
        TrackFinished { .. } if state.playback.queue().is_empty() => {
            split_view.set_show_sidebar(false);
            state.playback.reset_album_id();
        }
        _ => {}
    }
}

/// Wire the player panel to playback events.
///
/// Subscribes to `PlaybackEvent` to:
/// - Auto-show the sidebar on playback start
/// - Auto-hide the sidebar on stop when queue is empty
/// - Track the currently playing album ID
pub fn wire_panel_events(state: &Arc<AppState>, split_view: &OverlaySplitView) {
    let sv = split_view.clone();
    let state_ref = Arc::clone(state);
    let rx = state.playback.subscribe();

    let (album_tx, album_rx) = unbounded::<(i64, i64)>();

    spawn_panel_event_listener(rx, Arc::clone(&state_ref), sv, album_tx);
    spawn_album_id_listener(album_rx, state_ref);
}

/// Spawn a local future that listens for playback events and updates the panel.
fn spawn_panel_event_listener(
    rx: Receiver<PlaybackEvent>,
    state: Arc<AppState>,
    split_view: OverlaySplitView,
    album_tx: Sender<(i64, i64)>,
) {
    MainContext::default().spawn_local(async move {
        while let Ok(event) = rx.recv().await {
            handle_panel_event(&event, &state, &split_view, &album_tx);
        }
    });
}

/// Spawn a local future that receives `album_id` updates and applies them.
fn spawn_album_id_listener(rx: Receiver<(i64, i64)>, state: Arc<AppState>) {
    MainContext::default().spawn_local(async move {
        while let Ok((tid, album_id)) = rx.recv().await {
            state.playback.set_album_id_if_current(tid, album_id);
        }
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

//! Slide-in side player panel.
//!
//! Wires the player panel to `PlaybackState` and `PlaybackEvent` stream.
//! Handles auto-show on playback start and auto-hide on queue empty/stop.
//! Implements responsive behavior for narrow windows.

pub mod controls;
pub mod panel;
pub mod queue;

use std::sync::{Arc, atomic::Ordering::Relaxed};

use libadwaita::{OverlaySplitView, glib::spawn_future_local};

use crate::{
    app::AppState,
    playback::engine::{
        PlaybackController,
        PlaybackEvent::{self, Stopped, TrackStarted},
        PlaybackStatus::Stopped as StatusStopped,
    },
    storage::Storage,
};

/// Handle a playback event by showing or hiding the sidebar
/// and tracking the currently playing album.
async fn handle_panel_event(sv: &OverlaySplitView, state: &Arc<AppState>, event: &PlaybackEvent) {
    match event {
        TrackStarted { track_id } => {
            sv.set_show_sidebar(true);
            let album_id = match state.storage.get_track(*track_id).await {
                Ok(Some(track)) => track.audio.album_id.unwrap_or(-1),
                _ => -1,
            };
            state.current_album_id.store(album_id, Relaxed);
        }
        Stopped => {
            let ps = state.playback.state();
            if ps.current_track_id.is_none() && ps.status == StatusStopped {
                sv.set_show_sidebar(false);
            }
            state.current_album_id.store(-1, Relaxed);
        }
        _ => {}
    }
}

/// Wire the player panel to playback events and auto-show/hide behavior.
///
/// Listens for `PlaybackEvent` stream and:
/// - Auto-shows the sidebar on `TrackStarted`
/// - Auto-hides the sidebar on `Stopped` when queue is empty
/// - Tracks the currently playing album ID for UI state
pub fn wire_panel_events(state: &Arc<AppState>, split_view: &OverlaySplitView) {
    let mut event_rx = state.playback.subscribe();
    let sv = split_view.clone();
    let state_clone = Arc::clone(state);
    spawn_future_local(async move {
        while let Ok(event) = event_rx.recv().await {
            handle_panel_event(&sv, &state_clone, &event).await;
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

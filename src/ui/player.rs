//! Slide-in side player panel.
//!
//! Wires the player panel to `PlaybackEvent` stream.
//! Handles auto-show on playback start and auto-hide on queue empty/stop.
//! Implements responsive behavior for narrow windows.

pub mod acoustic_fader;
pub mod deck;
pub mod now_playing;
pub mod playback_events;
pub mod playlist;
pub mod row_factory;
pub mod sidebar;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        OverlaySplitView,
        glib::{MainContext, object::ObjectExt},
        gtk::{ToggleButton, prelude::ToggleButtonExt},
    },
    tokio::spawn,
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    metrics::GLOBAL_PANEL_REVEAL,
    playback::{
        state::PlaybackEvent::{self, Stopped, TrackFinished, TrackStarted},
        transport::PlaybackTransport,
    },
    storage::{Storage, database::SqliteStorage},
};

/// Fetch the album ID for a track and send it over the channel.
fn spawn_fetch_album_id(storage: Arc<SqliteStorage>, track_id: i64, tx: Sender<(i64, i64)>) {
    drop(spawn(async move {
        let album_id = match storage.get_track(track_id).await {
            Ok(Some(track)) => track.audio.album_id.unwrap_or(-1),
            _ => -1,
        };
        if let Err(e) = tx.try_send((track_id, album_id)) {
            error!(error = %e, "Failed to send album id");
        }
    }));
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
            GLOBAL_PANEL_REVEAL.record_visible();
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

/// Synchronize the sidebar toggle buttons with the split view.
///
/// Both toggle buttons reflect and drive the sidebar visibility, and an
/// unconsumed collapse notification restores the user's last intent.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `split_view` - Split view whose sidebar is synchronized.
/// * `toggle_button` - Header toggle button for the player panel.
/// * `back_button` - Sidebar back button mirroring the toggle state.
pub fn wire_sidebar_toggles(
    state: &Arc<AppState>,
    split_view: &OverlaySplitView,
    toggle_button: &ToggleButton,
    back_button: &ToggleButton,
) {
    let user_wants_sidebar = Arc::new(AtomicBool::new(false));

    let sv = split_view.clone();
    let intended = Arc::clone(&user_wants_sidebar);
    state
        .handles
        .lock()
        .retain_signal(toggle_button.connect_toggled(move |btn| {
            intended.store(btn.is_active(), Relaxed);
            if sv.shows_sidebar() != btn.is_active() {
                sv.set_show_sidebar(btn.is_active());
            }
        }));

    let sv_back = split_view.clone();
    let intended_back = Arc::clone(&user_wants_sidebar);
    state
        .handles
        .lock()
        .retain_signal(back_button.connect_toggled(move |btn| {
            intended_back.store(btn.is_active(), Relaxed);
            if sv_back.shows_sidebar() != btn.is_active() {
                sv_back.set_show_sidebar(btn.is_active());
            }
        }));

    let intended_collapse = Arc::clone(&user_wants_sidebar);
    state
        .handles
        .lock()
        .retain_signal(split_view.connect_notify(Some("collapsed"), move |sv, _| {
            if sv.is_collapsed() {
                return;
            }
            let wants = intended_collapse.load(Relaxed);
            if sv.shows_sidebar() != wants {
                sv.set_show_sidebar(wants);
            }
        }));
}

/// Spawn a local future that listens for playback events and updates the panel.
fn spawn_panel_event_listener(
    rx: Receiver<PlaybackEvent>,
    state: Arc<AppState>,
    split_view: OverlaySplitView,
    album_tx: Sender<(i64, i64)>,
) {
    let state_lock = Arc::clone(&state);
    state_lock
        .handles
        .lock()
        .retain_task(MainContext::default().spawn_local(async move {
            while let Ok(event) = rx.recv().await {
                handle_panel_event(&event, &state, &split_view, &album_tx);
            }
        }));
}

/// Spawn a local future that receives `album_id` updates and applies them.
fn spawn_album_id_listener(rx: Receiver<(i64, i64)>, state: Arc<AppState>) {
    let state_lock = Arc::clone(&state);
    state_lock
        .handles
        .lock()
        .retain_task(MainContext::default().spawn_local(async move {
            while let Ok((tid, album_id)) = rx.recv().await {
                state.playback.set_album_id_if_current(tid, album_id);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        async_channel::{Sender, unbounded},
        libadwaita::{
            OverlaySplitView,
            gtk::{self, test},
        },
        tokio::runtime::Runtime,
    };

    use crate::{
        app::runtime::AppState,
        playback::{
            state::{
                PlaybackEvent::{QueueChanged, Stopped, TrackFinished, TrackStarted},
                PlaybackState,
                PlaybackStatus::{Playing, Stopped as StatusStopped},
            },
            transport::PlaybackTransport,
        },
        ui::player::{handle_panel_event, wire_panel_events},
    };

    #[test]
    fn wire_panel_events_spawns_both_listeners() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let split_view = make_split_view();
        wire_panel_events(&state, &split_view);
        ensure!(
            format!("{:?}", state.handles.lock()).contains("tasks: 2"),
            "both panel listeners should be spawned"
        );
        Ok(())
    }

    #[test]
    fn empty_state_implies_queue_empty() {
        let state = PlaybackState::default();
        assert!(state.current_track_id.is_none());
        assert_eq!(state.status, StatusStopped);
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

    fn make_split_view() -> OverlaySplitView {
        OverlaySplitView::new()
    }

    fn make_album_tx() -> Sender<(i64, i64)> {
        unbounded::<(i64, i64)>().0
    }

    #[test]
    fn handle_panel_event_stopped_hides_sidebar() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        state.playback.shared.state.lock().current_album_id = 7;
        let split_view = make_split_view();
        split_view.set_show_sidebar(true);
        let album_tx = make_album_tx();
        handle_panel_event(&Stopped, &state, &split_view, &album_tx);
        ensure!(!split_view.shows_sidebar());
        ensure!(state.playback.state().current_album_id == -1);
        Ok(())
    }

    #[test]
    fn handle_panel_event_track_finished_hides_when_queue_empty() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let split_view = make_split_view();
        split_view.set_show_sidebar(true);
        let album_tx = make_album_tx();
        handle_panel_event(
            &TrackFinished { track_id: 1 },
            &state,
            &split_view,
            &album_tx,
        );
        ensure!(!split_view.shows_sidebar());
        Ok(())
    }

    #[test]
    fn handle_panel_event_queue_changed_leaves_sidebar() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let split_view = make_split_view();
        split_view.set_show_sidebar(false);
        let album_tx = make_album_tx();
        handle_panel_event(
            &QueueChanged { track_ids: vec![1] },
            &state,
            &split_view,
            &album_tx,
        );
        ensure!(!split_view.shows_sidebar());
        Ok(())
    }

    #[test]
    fn handle_panel_event_track_started_shows_sidebar() -> Result<()> {
        let rt = Runtime::new()?;
        let guard = rt.enter();
        let state = Arc::new(AppState::mock()?);
        let split_view = make_split_view();
        let album_tx = make_album_tx();
        handle_panel_event(
            &TrackStarted { track_id: 1 },
            &state,
            &split_view,
            &album_tx,
        );
        ensure!(split_view.shows_sidebar());
        drop(guard);
        Ok(())
    }
}

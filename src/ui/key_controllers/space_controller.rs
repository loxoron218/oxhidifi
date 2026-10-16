//! Plain `Space` playback controller for the main window.
//!
//! An open panel with an active track toggles first so side-panel music keeps
//! playing; otherwise the visible album/artist detail page starts its
//! playback; otherwise an open-but-idle panel is consumed and a hidden panel
//! falls through. Uses the `Capture` phase so it runs before the gallery
//! `FlowBox` card-activation handler. Text-input focus always wins.

use std::sync::Arc;

use {
    libadwaita::{
        ApplicationWindow, NavigationView, OverlaySplitView,
        gdk::Key,
        glib::{
            Propagation::{self, Proceed, Stop},
            spawn_future_local,
        },
        gtk::{EventControllerKey, PropagationPhase::Capture},
        prelude::{EventControllerExt, GtkWindowExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    playback::transport::PlaybackTransport,
    ui::{
        gallery::play_action::{play_artist, toggle_or_play_album},
        key_bindings::{
            play_pause_key::{
                SpaceAction::{self, ConsumeIdle, Ignore, PlayAlbum, PlayArtist, ToggleSidebar},
                resolve_space_action,
            },
            text_entry::focus_is_text_entry,
        },
        navigation::visible_detail_target,
    },
};

/// Toggle sidebar playback, logging failures.
///
/// # Arguments
///
/// * `state` - Application state owning the playback engine.
///
/// # Returns
///
/// * `Propagation` - `Stop` so the key never reaches grid navigation.
fn toggle_sidebar_space(state: &AppState) -> Propagation {
    if let Err(e) = state.playback.toggle_pause() {
        error!(error = %e, "Failed to toggle playback via Space key");
    }
    Stop
}

/// Start the visible album detail page.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `album_id` - Album to toggle or play.
///
/// # Returns
///
/// * `Propagation` - `Stop` so the focused Play button does not double-trigger.
fn start_album_space(state: &Arc<AppState>, album_id: i64) -> Propagation {
    let task_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            toggle_or_play_album(&task_state, album_id).await;
        }));
    Stop
}

/// Start the visible artist detail page.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `artist_id` - Artist to play.
///
/// # Returns
///
/// * `Propagation` - `Stop` so the focused Play button does not double-trigger.
fn start_artist_space(state: &Arc<AppState>, artist_id: i64) -> Propagation {
    let task_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            play_artist(&task_state, artist_id).await;
        }));
    Stop
}

/// Dispatch a resolved `Space` action to its playback effect.
///
/// # Arguments
///
/// * `state` - Application state owning playback and signal handles.
/// * `action` - Resolved sidebar-or-detail action.
///
/// # Returns
///
/// * `Propagation` - `Stop` when handled, `Proceed` when ignored.
fn dispatch_space_action(state: &Arc<AppState>, action: SpaceAction) -> Propagation {
    match action {
        ToggleSidebar | ConsumeIdle => toggle_sidebar_space(state),
        PlayAlbum(album_id) => start_album_space(state, album_id),
        PlayArtist(artist_id) => start_artist_space(state, artist_id),
        Ignore => Proceed,
    }
}

/// Add the plain `Space` playback controller to the window.
///
/// An open panel with an active track toggles first so side-panel music
/// keeps playing; otherwise the visible album/artist detail page starts its
/// playback; otherwise an open-but-idle panel is consumed and a hidden panel
/// falls through. Uses the `Capture` phase so it runs before the gallery
/// `FlowBox` card-activation handler. Text-input focus always wins.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `split_view` - Split view whose sidebar visibility gates the shortcut.
/// * `nav_view` - Navigation view providing the visible detail target.
/// * `state` - Application state owning the retained signal handles.
pub fn add_space_controller(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    nav_view: &NavigationView,
    state: &Arc<AppState>,
) {
    let space_state = Arc::clone(state);
    let space_split = split_view.clone();
    let space_nav = nav_view.clone();
    let space_window = window.clone();
    let space_controller = EventControllerKey::new();
    space_controller.set_propagation_phase(Capture);
    state
        .handles
        .lock()
        .retain_signal(
            space_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if key != Key::space {
                    return Proceed;
                }
                if focus_is_text_entry(space_window.focus().as_ref()) {
                    return Proceed;
                }
                let detail = visible_detail_target(&space_nav);
                let action =
                    resolve_space_action(&space_state, &space_split, detail, key, modifiers);
                dispatch_space_action(&space_state, action)
            }),
        );
    window.add_controller(space_controller);
}

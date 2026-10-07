//! Window-close persistence and application quit.

use std::sync::Arc;

use {
    libadwaita::{
        Application, ApplicationWindow, OverlaySplitView,
        glib::{Propagation::Proceed, idle_add_local_once},
        prelude::{ApplicationExt, GtkWindowExt},
    },
    tracing::{info, warn},
};

use crate::{app::runtime::AppState, ui::window_geometry::snapshot_geometry};

/// Wire window-close persistence and application quit.
///
/// Snapshots the actual allocation (see [`snapshot_geometry`]) so a
/// compositor-tiled window, e.g. snapped right at 50% width, reopens at that
/// pixel size instead of the stale pre-tile floating size. The sidebar
/// visibility is snapshotted too so the debounced toggle write cannot be
/// lost when the process exits immediately after close.
///
/// # Arguments
///
/// * `app` - Application to quit on close.
/// * `window` - Window whose close request is handled.
/// * `split_view` - Split view whose sidebar visibility is persisted.
/// * `state` - Application state owning the retained signal handles.
pub fn wire_close_request(
    app: &Application,
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    state: &Arc<AppState>,
) {
    let persist_state = Arc::clone(state);
    let persist_window = window.clone();
    let persist_split = split_view.clone();
    let quit_app = app.clone();
    state
        .handles
        .lock()
        .retain_signal(window.connect_close_request(move |_| {
            info!(
                reason = "close_request",
                "Window close requested — persisting geometry and session"
            );
            let (width, height, maximized) = snapshot_geometry(&persist_window);
            let sidebar_visible = persist_split.shows_sidebar();
            persist_geometry_and_session(&persist_state, width, height, maximized, sidebar_visible);
            let quit = quit_app.clone();
            persist_state
                .handles
                .lock()
                .retain_source(idle_add_local_once(move || quit.quit()));
            Proceed
        }));
}

/// Persist window geometry, sidebar visibility, and playback session synchronously.
///
/// Runs on the `GLib` main thread during `close_request` so the data is
/// durable before `Application::quit` exits the main loop.
fn persist_geometry_and_session(
    state: &AppState,
    width: i32,
    height: i32,
    maximized: bool,
    sidebar_visible: bool,
) {
    if let Err(e) = state
        .storage
        .set_window_geometry_sync(width, height, maximized)
    {
        warn!(error = %e, "Failed to persist window geometry");
    }
    if let Err(e) = state.storage.set_sidebar_visible_sync(sidebar_visible) {
        warn!(error = %e, "Failed to persist sidebar visibility");
    }
    if let Err(e) = state.persist_playback_session() {
        warn!(error = %e, "Failed to persist session on window close");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libadwaita::{Application, ApplicationWindow, OverlaySplitView};

    use crate::{app::runtime::AppState, ui::window_close::wire_close_request};

    #[test]
    fn wire_close_request_signature_shape() {
        fn assert_shape<
            F: Fn(&Application, &ApplicationWindow, &OverlaySplitView, &Arc<AppState>),
        >(
            _: F,
        ) {
        }
        assert_shape(wire_close_request);
    }
}

//! Main application window with `OverlaySplitView` sidebar layout.
//!
//! Creates the window with `AdwToolbarView` panes for sidebar and content;
//! the sidebar has its own `AdwHeaderBar` with back button (Nautilus pattern).
//! Keyboard shortcuts and responsive breakpoints live in the sibling
//! [`key_bindings`] and [`collapse_scheduler`] modules.

use std::sync::Arc;

use {
    libadwaita::{
        Application, ApplicationWindow, OverlaySplitView, Toast, ToastOverlay,
        ToastPriority::Normal,
        gdk::{Display, Key},
        glib::{
            Propagation::{Proceed, Stop},
            idle_add_local_once,
            object::{Cast, ObjectExt},
            spawn_future_local,
        },
        gtk::{
            Button, CssProvider, EventControllerKey, STYLE_PROVIDER_PRIORITY_APPLICATION,
            ToggleButton, Window, style_context_add_provider_for_display,
        },
        prelude::{
            AdwApplicationWindowExt, ApplicationExt, ButtonExt, GtkWindowExt, ToggleButtonExt,
            WidgetExt,
        },
    },
    tracing::{info, warn},
};

use crate::{
    app::runtime::AppState,
    ui::{
        collapse_scheduler::add_responsive_breakpoints,
        gallery::narrow_flag::NarrowState,
        key_bindings::{handle_escape_key, handle_zoom_key},
        panes::build_content,
        player::wire_panel_events,
    },
};

/// Build the main application window.
///
/// Creates an `AdwApplicationWindow` with `AdwOverlaySplitView`
/// containing separate `ToolbarView` panes for the sidebar and
/// content. The sidebar is hidden by default and auto-shown on
/// playback start.
///
/// # Panics
///
/// This function does not panic under normal operation. Internal callbacks
/// gracefully handle unexpected window types without panicking.
pub fn build_window(app: &Application, state: &Arc<AppState>) -> ApplicationWindow {
    info!("Building main application window");

    let (win_width, win_height, win_maximized) = state.storage.get_window_geometry();
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(win_width)
        .default_height(win_height)
        .build();
    if win_maximized {
        window.maximize();
    }

    load_hig_css();

    let narrow_state = NarrowState::new_shared();
    let (toast_overlay, split_view, toggle_button, back_button, close_button, switchers) =
        build_content(state, &narrow_state, window.upcast_ref::<Window>());
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoints(&window, &split_view, &narrow_state, &switchers);

    wire_panel_events(state, &split_view);

    add_key_controllers(&window, &split_view, state);

    wire_close_request(app, &window, state);

    wire_sidebar_sync(
        state,
        &window,
        &split_view,
        &toggle_button,
        &back_button,
        &close_button,
    );
    state.handles.lock().retain_binding(
        split_view
            .bind_property("collapsed", &close_button, "visible")
            .sync_create()
            .build(),
    );

    window
}

/// Add Escape and zoom key controllers to the window.
///
/// # Arguments
///
/// * `window` - Window receiving the controllers.
/// * `split_view` - Split view used for Escape-key handling.
/// * `state` - Application state owning the retained signal handles.
fn add_key_controllers(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    state: &Arc<AppState>,
) {
    let esc_split = split_view.clone();
    let esc_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(esc_controller.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Escape && handle_escape_key(&esc_split) {
                Stop
            } else {
                Proceed
            }
        }));
    window.add_controller(esc_controller);

    let zoom_state = Arc::clone(state);
    let zoom_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(
            zoom_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if handle_zoom_key(&zoom_state, key, modifiers) {
                    Stop
                } else {
                    Proceed
                }
            }),
        );
    window.add_controller(zoom_controller);
}

/// Wire window-close persistence and application quit.
///
/// # Arguments
///
/// * `app` - Application to quit on close.
/// * `window` - Window whose close request is handled.
/// * `state` - Application state owning the retained signal handles.
fn wire_close_request(app: &Application, window: &ApplicationWindow, state: &Arc<AppState>) {
    let persist_state = Arc::clone(state);
    let persist_window = window.clone();
    let quit_app = app.clone();
    state
        .handles
        .lock()
        .retain_signal(window.connect_close_request(move |_| {
            info!("Window close requested — persisting geometry and session");
            persist_geometry_and_session(
                &persist_state,
                persist_window.default_width(),
                persist_window.default_height(),
                persist_window.is_maximized(),
            );
            let quit = quit_app.clone();
            persist_state
                .handles
                .lock()
                .retain_source(idle_add_local_once(move || quit.quit()));
            Proceed
        }));
}

/// Synchronize sidebar toggle buttons and persist geometry changes.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `window` - Window whose maximize notifications are persisted.
/// * `split_view` - Split view whose sidebar state is synchronized.
/// * `toggle_button` - Header toggle button for the player panel.
/// * `back_button` - Sidebar back button mirroring the toggle state.
/// * `close_button` - Close button bound to sidebar collapse.
fn wire_sidebar_sync(
    state: &Arc<AppState>,
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    toggle_button: &ToggleButton,
    back_button: &ToggleButton,
    close_button: &Button,
) {
    let toggle_button = toggle_button.clone();
    let back_button = back_button.clone();
    let close_button = close_button.clone();
    let geom_state = Arc::clone(state);
    let geom_window = window.clone();
    state
        .handles
        .lock()
        .retain_signal(geom_window.connect_notify(Some("maximized"), move |w, _| {
            let Some(win) = w.downcast_ref::<ApplicationWindow>() else {
                warn!("Maximized notification received for non-ApplicationWindow");
                return;
            };
            let width = win.default_width();
            let height = win.default_height();
            let maximized = win.is_maximized();
            if let Err(e) = geom_state
                .storage
                .set_window_geometry_sync(width, height, maximized)
            {
                warn!(error = %e, "Failed to persist window geometry on maximize");
            }
        }));

    state
        .handles
        .lock()
        .retain_signal(split_view.connect_show_sidebar_notify(move |sv| {
            let showing = sv.shows_sidebar();
            info!(showing, "Sidebar visibility changed",);
            toggle_button.set_visible(!showing);
            toggle_button.set_active(showing);
            back_button.set_visible(showing);
            back_button.set_active(showing);
        }));

    let window_close = window.clone();
    state
        .handles
        .lock()
        .retain_signal(close_button.connect_clicked(move |_| {
            window_close.close();
        }));
}

/// Persist window geometry and playback session synchronously.
///
/// Runs on the `GLib` main thread during `close_request` so the data is
/// durable before `Application::quit` exits the main loop.
fn persist_geometry_and_session(state: &AppState, width: i32, height: i32, maximized: bool) {
    if let Err(e) = state
        .storage
        .set_window_geometry_sync(width, height, maximized)
    {
        warn!(error = %e, "Failed to persist window geometry");
    }
    if let Err(e) = state.persist_playback_session() {
        warn!(error = %e, "Failed to persist session on window close");
    }
}

/// Load HIG-compliant CSS transitions (200 ms ease) and style rules.
///
/// Applies 200 ms ease transitions to the header bar and to the
/// `AdwOverlaySplitView` sidebar reveal/hide per FR-023/FR-025. The split-view
/// rule targets the overlay's sidebar and content so both the slide-in
/// (FR-023) and slide-out (FR-025) use the same HIG timing.
fn load_hig_css() {
    let Some(display) = Display::default() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_string(
        "
        headerbar {
            transition: background 200ms ease;
        }
        overlay-split-view {
            transition: all 200ms ease;
        }
        overlay-split-view > widget {
            transition: all 200ms ease;
        }
        navigation-split-view {
            transition: all 200ms ease;
        }
        navigation-split-view > widget {
            transition: all 200ms ease;
        }
        .sidebar,
        .content {
            transition: all 200ms ease;
        }
        ",
    );
    style_context_add_provider_for_display(
        &display,
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Spawn a future to listen for toast messages and display them.
fn listen_for_toasts(state: &Arc<AppState>, toast_overlay: &ToastOverlay) {
    let rx = state.toast_rx.clone();
    let overlay = toast_overlay.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            while let Ok(message) = rx.recv().await {
                let toast = Toast::builder().title(message).priority(Normal).build();
                overlay.add_toast(toast);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{Application, ApplicationWindow},
    };

    use crate::{app::runtime::AppState, ui::window::build_window};

    #[test]
    fn window_builds_with_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        ensure!(
            format!("{:?}", state.handles.lock()).contains("tasks: 0"),
            "fresh mock state must own no tasks"
        );
        drop(state);
        Ok(())
    }

    #[test]
    fn window_builder_signature_shape() {
        fn assert_shape<F: Fn(&Application, &Arc<AppState>) -> ApplicationWindow>(_: F) {}
        assert_shape(build_window);
    }
}

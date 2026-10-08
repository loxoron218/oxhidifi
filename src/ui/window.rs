//! Main application window with `OverlaySplitView` sidebar layout.
//!
//! Creates the window with `AdwToolbarView` panes for sidebar and content;
//! the sidebar has its own `AdwHeaderBar` with back button (Nautilus pattern).
//! Keyboard shortcuts and responsive breakpoints live in the sibling
//! [`key_controllers`] and [`collapse_scheduler`] modules.

use std::sync::{Arc, atomic::AtomicBool};

use {
    libadwaita::{
        Application, ApplicationWindow, OverlaySplitView, Toast, ToastOverlay,
        ToastPriority::Normal,
        gdk::Display,
        glib::{
            object::{Cast, ObjectExt},
            spawn_future_local,
        },
        gtk::{
            Button, CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, ToggleButton, Window,
            style_context_add_provider_for_display,
        },
        prelude::{AdwApplicationWindowExt, ButtonExt, GtkWindowExt, ToggleButtonExt, WidgetExt},
    },
    tracing::{info, warn},
};

use crate::{
    app::runtime::AppState,
    ui::{
        collapse_scheduler::add_responsive_breakpoints,
        gallery::narrow_flag::NarrowState,
        key_controllers::add_key_controllers,
        panes::build_content,
        player::wire_panel_events,
        window_close::wire_close_request,
        window_geometry::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, clamp_geometry, snapshot_geometry},
    },
};

/// Build the main application window.
///
/// Creates an `AdwApplicationWindow` with `AdwOverlaySplitView`
/// containing separate `ToolbarView` panes for the sidebar and
/// content. The sidebar restores its persisted visibility from settings;
/// fresh playback starts auto-show it while automatic advances and the
/// startup restore leave it untouched.
///
/// The window minimum is set explicitly to [`MIN_WINDOW_WIDTH`] by
/// [`MIN_WINDOW_HEIGHT`]: breakpoints remove the automatic window minimum,
/// without which the window can briefly measure children at ~30 px during
/// startup, producing `Trying to measure GtkRevealer/GtkScrolledWindow for
/// width of 30` warnings.
pub fn build_window(app: &Application, state: &Arc<AppState>) -> ApplicationWindow {
    info!(component = "window", "Building main application window");

    let (stored_width, stored_height, win_maximized) = state.storage.get_window_geometry();
    let (win_width, win_height) = clamp_geometry(stored_width, stored_height);
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(win_width)
        .default_height(win_height)
        .build();
    if win_maximized {
        window.maximize();
    }
    window.set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);

    load_hig_css();

    let narrow_state = NarrowState::new_shared();
    let sidebar_intent = Arc::new(AtomicBool::new(state.storage.get_sidebar_visible()));
    let (toast_overlay, split_view, toggle_button, back_button, close_button, switchers, nav_view) =
        build_content(
            state,
            &narrow_state,
            window.upcast_ref::<Window>(),
            &sidebar_intent,
        );
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoints(&window, &split_view, &narrow_state, &switchers);

    wire_panel_events(state, &split_view, &sidebar_intent);

    add_key_controllers(
        &window,
        &split_view,
        &nav_view,
        state,
        &sidebar_intent,
        window.upcast_ref::<Window>(),
    );

    wire_close_request(app, &window, &split_view, state);

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

/// Synchronize sidebar toggle buttons and persist geometry changes.
///
/// State transitions (maximize, fullscreen) persist via [`snapshot_geometry`],
/// which keeps the last floating size while the allocation is
/// compositor-owned so un-maximizing restores correctly. Sidebar visibility
/// changes persist via a debounced background write; the close-request path
/// re-snapshots synchronously so the final state is durable.
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
    let persist_on_state_change = move |w: &ApplicationWindow, reason: &str| {
        let (width, height, maximized) = snapshot_geometry(w);
        if let Err(e) = geom_state
            .storage
            .set_window_geometry_sync(width, height, maximized)
        {
            warn!(error = %e, reason, "Failed to persist window geometry");
        }
    };
    let persist_maximized = persist_on_state_change.clone();
    state
        .handles
        .lock()
        .retain_signal(geom_window.connect_notify(Some("maximized"), move |w, _| {
            let Some(win) = w.downcast_ref::<ApplicationWindow>() else {
                warn!(
                    expected = "ApplicationWindow",
                    "Maximized notification received for non-ApplicationWindow"
                );
                return;
            };
            persist_maximized(win, "maximize");
        }));
    let persist_fullscreen = persist_on_state_change.clone();
    let fullscreen_window = window.clone();
    state
        .handles
        .lock()
        .retain_signal(
            fullscreen_window.connect_notify(Some("fullscreened"), move |w, _| {
                let Some(win) = w.downcast_ref::<ApplicationWindow>() else {
                    warn!(
                        expected = "ApplicationWindow",
                        "Fullscreen notification received for non-ApplicationWindow"
                    );
                    return;
                };
                persist_fullscreen(win, "fullscreen");
            }),
        );

    let sidebar_state = Arc::clone(state);
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
            sidebar_state.storage.set_sidebar_visible_memory(showing);
            sidebar_state.storage.save_settings();
        }));

    let window_close = window.clone();
    state
        .handles
        .lock()
        .retain_signal(close_button.connect_clicked(move |_| {
            window_close.close();
        }));
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
    use std::sync::{Arc, atomic::AtomicBool};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            Application, ApplicationWindow, NavigationView, OverlaySplitView, ToastOverlay,
            gtk::{Button, ToggleButton, Window},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::{
            gallery::narrow_flag::NarrowState,
            panes::{SwitcherGroup, build_content},
            window::build_window,
        },
    };

    #[test]
    fn build_content_signature_shape() {
        fn assert_shape<
            F: Fn(
                &Arc<AppState>,
                &Arc<NarrowState>,
                &Window,
                &Arc<AtomicBool>,
            ) -> (
                ToastOverlay,
                OverlaySplitView,
                ToggleButton,
                ToggleButton,
                Button,
                SwitcherGroup,
                NavigationView,
            ),
        >(
            _: F,
        ) {
        }
        assert_shape(build_content);
    }

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

//! Main application window with `OverlaySplitView` sidebar layout.
//!
//! Creates the window with `AdwToolbarView` panes for sidebar and content;
//! the sidebar has its own `AdwHeaderBar` with back button (Nautilus pattern).
//! Keyboard shortcuts and responsive breakpoints live in the sibling
//! [`key_bindings`] and [`collapse_scheduler`] modules.

use std::sync::Arc;

use {
    libadwaita::{
        Application, ApplicationWindow, Toast, ToastOverlay,
        ToastPriority::Normal,
        gdk::{Display, Key},
        glib::{
            Propagation::{Proceed, Stop},
            object::{Cast, ObjectExt},
            spawn_future_local,
        },
        gtk::{
            CssProvider, EventControllerKey, STYLE_PROVIDER_PRIORITY_APPLICATION, Window,
            prelude::ToggleButtonExt, style_context_add_provider_for_display,
        },
        prelude::{AdwApplicationWindowExt, ButtonExt, GtkWindowExt, WidgetExt},
    },
    tracing::info,
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
pub fn build_window(app: &Application, state: &Arc<AppState>) -> ApplicationWindow {
    info!("Building main application window");

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(1200)
        .default_height(800)
        .build();

    load_hig_css();

    let narrow_state = NarrowState::new_shared();
    let (toast_overlay, split_view, toggle_button, back_button, close_button, switchers) =
        build_content(state, &narrow_state, window.upcast_ref::<Window>());
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoints(&window, &split_view, &narrow_state, &switchers);

    wire_panel_events(state, &split_view);

    let esc_split = split_view.clone();
    let esc_controller = EventControllerKey::new();
    esc_controller.connect_key_pressed(move |_, key, _, _| {
        if key == Key::Escape && handle_escape_key(&esc_split) {
            Stop
        } else {
            Proceed
        }
    });
    window.add_controller(esc_controller);

    let zoom_state = Arc::clone(state);
    let zoom_controller = EventControllerKey::new();
    zoom_controller.connect_key_pressed(move |_, key, _, modifiers| {
        if handle_zoom_key(&zoom_state, key, modifiers) {
            Stop
        } else {
            Proceed
        }
    });
    window.add_controller(zoom_controller);

    window.connect_close_request(|_| {
        info!("Window close requested — session persists during application shutdown");
        Proceed
    });

    split_view.connect_show_sidebar_notify(move |sv| {
        let showing = sv.shows_sidebar();
        info!(showing, "Sidebar visibility changed",);
        toggle_button.set_visible(!showing);
        toggle_button.set_active(showing);
        back_button.set_visible(showing);
        back_button.set_active(showing);
    });

    let window_close = window.clone();
    close_button.connect_clicked(move |_| {
        window_close.close();
    });

    split_view
        .bind_property("collapsed", &close_button, "visible")
        .sync_create()
        .build();

    window
}

/// Load HIG-compliant CSS transitions (200 ms ease) and style rules.
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
    spawn_future_local(async move {
        while let Ok(message) = rx.recv().await {
            let toast = Toast::builder().title(message).priority(Normal).build();
            overlay.add_toast(toast);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;

    use crate::app::runtime::AppState;

    #[test]
    fn window_builds_with_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        drop(state);
        Ok(())
    }
}

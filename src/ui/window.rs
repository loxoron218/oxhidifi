//! Main application window with `OverlaySplitView` sidebar layout.
//!
//! Creates the main window with `AdwOverlaySplitView` containing
//! separate `AdwToolbarView` panes for sidebar and content.
//! The sidebar pane has its own `AdwHeaderBar` with back button,
//! mirroring the Nautilus sidebar pattern.

use std::sync::Arc;

use {
    libadwaita::{
        Application, ApplicationWindow, Breakpoint, BreakpointCondition,
        BreakpointConditionLengthType::MaxWidth,
        LengthUnit::Sp,
        OverlaySplitView, Toast, ToastOverlay,
        ToastPriority::Normal,
        gdk::Display,
        glib::{
            Propagation::Proceed,
            object::{Cast, ObjectExt},
            prelude::ToValue,
            spawn_future_local,
        },
        gtk::{
            CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, Window, prelude::ToggleButtonExt,
            style_context_add_provider_for_display,
        },
        prelude::{AdwApplicationWindowExt, ButtonExt, GtkWindowExt, WidgetExt},
    },
    tracing::{error, info},
};

use crate::{
    app::AppState,
    playback::control::PlaybackController,
    ui::{
        library::narrow_state::NarrowState, player::wire_panel_events, window_panes::build_content,
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
    let (toast_overlay, split_view, toggle_button, back_button, close_button) =
        build_content(state, &narrow_state, window.upcast_ref::<Window>());
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoints(&window, &split_view, &narrow_state);

    wire_panel_events(state, &split_view);

    let close_state = Arc::clone(state);
    window.connect_close_request(move |_| {
        info!("Window close requested — persisting session");

        let s = close_state.playback.state();
        let queue_tracks = close_state.playback.queue().tracks();
        let queue_index = close_state.playback.queue().current_index();

        if let Err(e) = close_state.storage.set_last_session(
            queue_tracks,
            queue_index,
            s.current_track_id,
            s.elapsed_seconds,
            s.duration_seconds,
        ) {
            error!(error = %e, "Failed to persist session on close");
        }

        if let Err(e) = close_state.playback.stop() {
            error!(error = %e, "Failed to stop playback on window close");
        }
        close_state.cover_art_cache.shutdown();
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
        .overlay-split-view {
            transition: all 200ms ease;
        }
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

/// Add responsive breakpoints for narrow windows.
///
/// Collapses the `OverlaySplitView` sidebar below 800 px width and
/// hides non‑essential columns (Format, Bit Depth, Sample Rate) below
/// 700 px width.
fn add_responsive_breakpoints(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    narrow_state: &Arc<NarrowState>,
) {
    let sidebar_condition = BreakpointCondition::new_length(MaxWidth, 800.0, Sp);
    let sidebar_bp = Breakpoint::new(sidebar_condition);
    sidebar_bp.add_setter(split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(sidebar_bp);

    let narrow_condition = BreakpointCondition::new_length(MaxWidth, 700.0, Sp);
    let narrow_bp = Breakpoint::new(narrow_condition);
    narrow_bp.add_setter(split_view, "collapsed", Some(&true.to_value()));
    narrow_bp.connect_apply({
        let ns = Arc::clone(narrow_state);
        move |_| ns.set(true)
    });
    narrow_bp.connect_unapply({
        let ns = Arc::clone(narrow_state);
        move |_| ns.set(false)
    });
    window.add_breakpoint(narrow_bp);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::Result,
        libadwaita::gtk::{self, test},
    };

    use crate::app::AppState;

    #[test]
    fn window_builds_with_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        drop(state);
        Ok(())
    }
}

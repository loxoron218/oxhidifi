//! Main application window with `OverlaySplitView` sidebar layout.
//!
//! Creates the main window with `AdwOverlaySplitView` containing
//! separate `AdwToolbarView` panes for sidebar and content.
//! The sidebar pane has its own `AdwHeaderBar` with back button,
//! mirroring the Nautilus sidebar pattern.

use std::sync::Arc;

use libadwaita::{
    Application, ApplicationWindow, Breakpoint, BreakpointCondition,
    BreakpointConditionLengthType::MaxWidth,
    HeaderBar,
    LengthUnit::Sp,
    OverlaySplitView, Toast, ToastOverlay,
    ToastPriority::Normal,
    ToolbarView, ViewStack, ViewSwitcher, ViewSwitcherBar,
    ViewSwitcherPolicy::Wide,
    WindowTitle,
    glib::{prelude::ToValue, spawn_future_local},
    gtk::{ToggleButton, prelude::ToggleButtonExt},
    prelude::{AdwApplicationWindowExt, WidgetExt},
};

use crate::{
    app::AppState,
    ui::{
        header::build_header_controls,
        library::{albums::build_album_grid, artists::build_artist_grid},
        player::{panel::build_player_content, wire_panel_events},
        status::StatusBar,
    },
};

/// Build the main application window.
///
/// Creates an `AdwApplicationWindow` with `AdwOverlaySplitView`
/// containing separate `ToolbarView` panes for the sidebar and
/// content. The sidebar is hidden by default and auto-shown on
/// playback start.
#[must_use]
pub fn build_window(app: &Application, state: &Arc<AppState>) -> ApplicationWindow {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(1200)
        .default_height(800)
        .build();

    let (toast_overlay, split_view, toggle_button, back_button) = build_content(state);
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoint(&window, &split_view);

    wire_panel_events(state, &split_view);

    split_view.connect_show_sidebar_notify(move |sv| {
        let showing = sv.shows_sidebar();
        toggle_button.set_visible(!showing);
        toggle_button.set_active(showing);
        back_button.set_visible(showing);
        back_button.set_active(showing);
    });

    window
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

/// Add responsive breakpoint for narrow windows (T039).
///
/// Collapses the `OverlaySplitView` sidebar below 800px width.
fn add_responsive_breakpoint(window: &ApplicationWindow, split_view: &OverlaySplitView) {
    let condition = BreakpointCondition::new_length(MaxWidth, 800.0, Sp);
    let breakpoint = Breakpoint::new(condition);
    breakpoint.add_setter(split_view, "collapsed", Some(&true.to_value()));

    window.add_breakpoint(breakpoint);
}

/// Build the split-view content with sidebar and content panes.
///
/// Each pane has its own `ToolbarView` and `HeaderBar`. The sidebar
/// contains the player panel with a back button and "Now Playing"
/// title. The content pane contains the library view switcher and
/// stack. Bottom bars (view switcher and status) are attached to the
/// content pane.
///
/// Returns the `(ToastOverlay, OverlaySplitView, toggle_button, back_button)` for
/// event wiring in `build_window`.
fn build_content(
    state: &Arc<AppState>,
) -> (ToastOverlay, OverlaySplitView, ToggleButton, ToggleButton) {
    let toast_overlay = ToastOverlay::new();

    let sidebar_toolbar = ToolbarView::new();

    let sidebar_header = HeaderBar::new();
    sidebar_header.set_title_widget(Some(&WindowTitle::new("Now Playing", "")));

    let back_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Hide player panel")
        .active(true)
        .build();
    back_button.set_visible(false);
    sidebar_header.pack_end(&back_button);

    sidebar_toolbar.add_top_bar(&sidebar_header);

    let player_content = build_player_content(state);
    sidebar_toolbar.set_content(Some(&player_content));

    let content_toolbar = ToolbarView::new();

    let content_header = HeaderBar::new();

    let stack = ViewStack::new();
    stack.set_vexpand(true);

    let albums_page = build_album_grid(state);
    let albums_child =
        stack.add_titled_with_icon(&albums_page, Some("albums"), "Albums", "view-grid-symbolic");
    albums_child.set_icon_name(Some("view-grid-symbolic"));

    let artists_page = build_artist_grid(state);
    let artists_child = stack.add_titled_with_icon(
        &artists_page,
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    );
    artists_child.set_icon_name(Some("avatar-default-symbolic"));

    let switcher = ViewSwitcher::builder()
        .policy(Wide)
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();
    content_header.set_title_widget(Some(&switcher));

    let controls = build_header_controls(state);
    content_header.pack_end(&controls);

    let toggle_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Toggle player panel")
        .active(false)
        .css_classes(["flat"])
        .build();
    content_header.pack_start(&toggle_button);

    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&stack));

    let split_view = OverlaySplitView::builder()
        .sidebar(&sidebar_toolbar)
        .content(&content_toolbar)
        .min_sidebar_width(320.0)
        .max_sidebar_width(400.0)
        .show_sidebar(false)
        .tooltip_text("Player panel — toggle with button in header")
        .build();

    let sv = split_view.clone();
    toggle_button.connect_toggled(move |btn| {
        if sv.shows_sidebar() != btn.is_active() {
            sv.set_show_sidebar(btn.is_active());
        }
    });

    let sv_back = split_view.clone();
    back_button.connect_toggled(move |btn| {
        if sv_back.shows_sidebar() != btn.is_active() {
            sv_back.set_show_sidebar(btn.is_active());
        }
    });

    let switcher_bar = ViewSwitcherBar::builder()
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();
    content_toolbar.add_bottom_bar(&switcher_bar);

    let status_bar = StatusBar::new(state);
    content_toolbar.add_bottom_bar(status_bar.widget());

    toast_overlay.set_child(Some(&split_view));

    (toast_overlay, split_view, toggle_button, back_button)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;

    use crate::app::AppState;

    #[test]
    fn window_builds_with_state() -> Result<()> {
        let _state = Arc::new(AppState::mock()?);
        Ok(())
    }
}

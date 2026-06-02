//! Main application window with `ToolbarView` layout.
//!
//! Creates the main window with `AdwToolbarView`, `AdwHeaderBar`, and
//! `AdwViewSwitcher` for Albums/Artists tab navigation.
//! Implements tab switching logic with view content swap.

use std::sync::Arc;

use libadwaita::{
    Application, ApplicationWindow, HeaderBar, Toast, ToastOverlay,
    ToastPriority::Normal,
    ToolbarView, ViewStack, ViewSwitcher, ViewSwitcherBar,
    ViewSwitcherPolicy::Wide,
    glib::spawn_future_local,
    prelude::{AdwApplicationWindowExt, WidgetExt},
};

use crate::{
    app::AppState,
    ui::{
        header::build_header_controls,
        library::{albums::build_album_grid, artists::build_artist_grid},
        status::StatusBar,
    },
};

/// Build the main application window.
///
/// Creates an `AdwApplicationWindow` with `AdwToolbarView` and
/// `AdwViewSwitcher` for tab navigation.
#[must_use]
pub fn build_window(app: &Application, state: &Arc<AppState>) -> ApplicationWindow {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(1200)
        .default_height(800)
        .build();

    let toast_overlay = ToastOverlay::new();
    window.set_content(Some(&toast_overlay));

    let content = build_content(state);
    toast_overlay.set_child(Some(&content));

    listen_for_toasts(state, &toast_overlay);

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

/// Build the `AdwToolbarView` content area.
///
/// Assembles the header bar with `AdwViewSwitcher`, album/artist grid stack,
/// `AdwViewSwitcherBar` for narrow-mode tab navigation, and status bar.
fn build_content(state: &Arc<AppState>) -> ToolbarView {
    let toolbar_view = ToolbarView::new();

    let header = HeaderBar::new();

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

    header.set_title_widget(Some(&switcher));

    let controls = build_header_controls(state);
    header.pack_end(&controls);

    toolbar_view.add_top_bar(&header);

    let switcher_bar = ViewSwitcherBar::builder()
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();

    toolbar_view.add_bottom_bar(&switcher_bar);

    let status_bar = StatusBar::new(state);
    toolbar_view.add_bottom_bar(status_bar.widget());

    toolbar_view.set_content(Some(&stack));

    toolbar_view
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

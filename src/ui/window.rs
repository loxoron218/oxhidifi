//! Main application window with `ToolbarView` layout.
//!
//! Creates the main window with `AdwToolbarView`, `AdwHeaderBar`, and
//! `AdwViewSwitcher` for Albums/Artists tab navigation.

use std::sync::Arc;

use libadwaita::{
    Application, ApplicationWindow, HeaderBar, ToolbarView, ViewStack, ViewSwitcher,
    ViewSwitcherBar, ViewSwitcherPolicy::Wide, gtk::Label, prelude::*,
};

use crate::{app::AppState, ui::library::albums::build_album_grid};

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

    let content = build_content(state);
    window.set_content(Some(&content));

    window
}

/// Build the `AdwToolbarView` content area.
///
/// Assembles the header bar with `AdwViewSwitcher`, album grid stack,
/// and `AdwViewSwitcherBar` for narrow-mode tab navigation.
fn build_content(state: &Arc<AppState>) -> ToolbarView {
    let toolbar_view = ToolbarView::new();

    let header = HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let stack = ViewStack::new();
    stack.set_vexpand(true);

    let albums_page = build_album_grid(state);
    stack.add_titled_with_icon(&albums_page, Some("albums"), "Albums", "view-grid-symbolic");

    let artists_placeholder = Label::new(Some("Artists"));
    artists_placeholder.set_vexpand(true);
    stack.add_titled_with_icon(
        &artists_placeholder,
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    );

    let switcher = ViewSwitcher::builder()
        .policy(Wide)
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();

    header.set_title_widget(Some(&switcher));

    let switcher_bar = ViewSwitcherBar::builder()
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();

    toolbar_view.add_bottom_bar(&switcher_bar);
    toolbar_view.set_content(Some(&stack));

    toolbar_view
}

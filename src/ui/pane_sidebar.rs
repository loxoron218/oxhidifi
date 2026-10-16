//! Sidebar panel with player content for the main window shell.

use std::sync::Arc;

use libadwaita::{
    HeaderBar, ToolbarView, WindowTitle,
    gtk::{Button, ToggleButton, accessible::Property::Label},
    prelude::AccessibleExtManual,
};

use crate::{app::runtime::AppState, ui::player::sidebar::build_player_content};

/// Build the sidebar panel with player content.
///
/// # Arguments
///
/// * `state` - Application state owning storage and signal handles.
/// * `back_button` - Sidebar toggle button for the player panel.
///
/// # Returns
///
/// Sidebar toolbar, collapsed-mode close button, and owning header.
pub fn build_sidebar(
    state: &Arc<AppState>,
    back_button: &ToggleButton,
) -> (ToolbarView, Button, HeaderBar) {
    let sidebar_toolbar = ToolbarView::builder()
        .css_classes(["sidebar"])
        .tooltip_text("Player sidebar")
        .build();
    sidebar_toolbar.update_property(&[Label("Player sidebar")]);

    let close_button = Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close application")
        .css_classes(["flat", "image-button"])
        .build();
    close_button.update_property(&[Label("Close application")]);

    let sidebar_header = HeaderBar::builder()
        .css_classes(["sidebar-header"])
        .tooltip_text("Now playing header")
        .build();
    let window_title = WindowTitle::builder()
        .title("Now Playing")
        .subtitle("")
        .css_classes(["sidebar-title"])
        .tooltip_text("Now playing")
        .build();
    window_title.update_property(&[Label("Now playing")]);
    sidebar_header.set_title_widget(Some(&window_title));
    sidebar_header.pack_end(&close_button);
    sidebar_header.pack_end(back_button);

    sidebar_toolbar.add_top_bar(&sidebar_header);

    let player_content = build_player_content(state);
    sidebar_toolbar.set_content(Some(&player_content));

    (sidebar_toolbar, close_button, sidebar_header)
}

/// Position the sidebar toggle based on close-button visibility.
///
/// Packs at the far left when the close button is visible (collapsed
/// overlay mode), otherwise back at the right.
///
/// # Arguments
///
/// * `header` - Sidebar header bar owning both buttons.
/// * `back_button` - Sidebar toggle button to reposition.
/// * `close_visible` - Whether the close button is visible.
pub fn place_sidebar_toggle(header: &HeaderBar, back_button: &ToggleButton, close_visible: bool) {
    header.remove(back_button);
    if close_visible {
        header.pack_start(back_button);
    } else {
        header.pack_end(back_button);
    }
}

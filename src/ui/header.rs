//! `HeaderBar` with Albums/Artists tab buttons and view toggle controls.
//!
//! Uses `AdwViewSwitcher` for tab navigation per GNOME HIG. The switcher
//! is placed in the title widget slot of `AdwHeaderBar`.
//!
//! Provides a toggle button to switch between grid and column layout views
//! and a preferences button to open the settings dialog.

use std::sync::Arc;

use {
    libadwaita::{
        glib::spawn_future_local,
        gtk::{Box, Button, Orientation::Horizontal, ToggleButton, Window},
        prelude::{BoxExt, ButtonExt, ToggleButtonExt, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::AppState,
    storage::settings::ViewMode::{self, Column, Grid},
    ui::settings::show_preferences_dialog,
};

/// Persist the view mode setting to storage, logging on failure.
async fn save_view_mode(state: Arc<AppState>, mode: ViewMode) {
    if let Err(err) = state.storage.set_view_mode(mode).await {
        warn!(error = %err, "Failed to set view mode");
    }
}

/// Build the view mode toggle button.
///
/// Creates a `ToggleButton` that switches between grid and column layout.
/// The button icon updates to reflect the current view mode.
/// The button's `active` state is synced with the initial view mode
/// so the first click always toggles modes (no redundant no-op toggle).
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings
/// * `initial_mode` - The initial view mode to display
///
/// # Returns
///
/// A `ToggleButton` that toggles the view mode.
#[must_use]
pub fn build_view_toggle(state: &Arc<AppState>, initial_mode: ViewMode) -> ToggleButton {
    let toggle = ToggleButton::builder()
        .icon_name(initial_mode.icon_name())
        .tooltip_text(initial_mode.tooltip())
        .active(initial_mode == Column)
        .can_focus(true)
        .css_classes(["flat"])
        .build();

    let state_clone = Arc::clone(state);
    toggle.connect_toggled(move |btn| {
        let mode = if btn.is_active() { Column } else { Grid };
        btn.set_icon_name(mode.icon_name());
        btn.set_tooltip_text(Some(mode.tooltip()));
        let sc = Arc::clone(&state_clone);
        spawn_future_local(save_view_mode(sc, mode));
        if let Err(e) = state_clone.view_mode_tx.send(mode) {
            warn!(error = %e, "Failed to send view mode change");
        }
    });

    toggle
}

/// Build a header bar with view toggle and preferences button.
///
/// Creates a horizontal box containing the view toggle button and a
/// gear icon button to open the preferences dialog.
#[must_use]
pub fn build_header_controls(state: &Arc<AppState>, parent: &Window) -> Box {
    let controls = Box::builder().orientation(Horizontal).spacing(6).build();

    let initial_mode = state.storage.get_view_mode();

    let toggle = build_view_toggle(state, initial_mode);
    controls.append(&toggle);

    let prefs_btn = Button::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Preferences")
        .css_classes(["flat"])
        .can_focus(true)
        .build();

    let state_prefs = Arc::clone(state);
    let parent_clone = parent.clone();
    prefs_btn.connect_clicked(move |_| {
        show_preferences_dialog(&state_prefs, &parent_clone);
    });

    controls.append(&prefs_btn);

    controls
}

#[cfg(test)]
mod tests {
    use crate::storage::settings::ViewMode::{Column, Grid};

    #[test]
    fn view_mode_icon_names() {
        assert_eq!(Grid.icon_name(), "view-grid-symbolic");
        assert_eq!(Column.icon_name(), "view-list-symbolic");
    }

    #[test]
    fn view_mode_tooltips() {
        assert_eq!(Grid.tooltip(), "Switch to column view");
        assert_eq!(Column.tooltip(), "Switch to grid view");
    }
}

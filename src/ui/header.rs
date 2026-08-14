//! `HeaderBar` with Albums/Artists tab buttons and view toggle controls.
//!
//! Uses `AdwViewSwitcher` for tab navigation per GNOME HIG. The switcher
//! is placed in the title widget slot of `AdwHeaderBar`.
//!
//! Provides a `SplitButton` to toggle between grid and column layout views
//! with a popover containing zoom controls and sort configuration. The
//! popover construction lives in the sibling [`toggle_popover`] module.

use std::sync::Arc;

use {
    libadwaita::{
        SplitButton,
        glib::spawn_future_local,
        gtk::{
            Box, Button, Orientation::Horizontal, Widget, Window,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::Albums,
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::{preferences::show_preferences_dialog, toggle_popover::build_popover},
};

/// Persist the view mode setting to storage, logging on failure.
async fn save_view_mode(state: Arc<AppState>, mode: ViewMode) {
    if let Err(err) = state.storage.set_view_mode(mode).await {
        warn!(error = %err, "Failed to set view mode");
    }
}

/// Build the view mode toggle split button with popover.
///
/// Creates a `SplitButton` that switches between grid and column layout
/// on main button click. The arrow dropdown shows a popover with zoom
/// controls (zoom out/in) and sort configuration lists (albums/artists).
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings
///
/// # Returns
///
/// A `SplitButton` with attached popover for view and sort controls.
pub fn build_view_toggle(state: &Arc<AppState>) -> SplitButton {
    let initial_mode = state.storage.get_view_mode();

    let split_btn = SplitButton::builder()
        .icon_name(initial_mode.icon_name())
        .tooltip_text("Toggle View")
        .can_focus(true)
        .build();
    split_btn.update_property(&[PropertyLabel(initial_mode.tooltip())]);

    let (popover, albums_sort, artists_sort) = build_popover(state);
    split_btn.set_popover(Some(&popover));

    let state_clone = Arc::clone(state);
    split_btn.connect_clicked(move |btn| {
        let current_mode = state_clone.storage.get_view_mode();
        let mode = if current_mode == Grid { Column } else { Grid };
        btn.set_icon_name(mode.icon_name());
        btn.set_tooltip_text(Some(mode.tooltip()));
        let sc = Arc::clone(&state_clone);
        spawn_future_local(save_view_mode(sc, mode));
        state_clone.view_mode.send(mode);
    });

    subscribe_view_updates(state, &split_btn, &albums_sort, &artists_sort);

    split_btn
}

/// Subscribe to view mode and active tab changes to update split button icon
/// and toggle sort list visibility.
fn subscribe_view_updates(
    state: &Arc<AppState>,
    split_btn: &SplitButton,
    albums_sort: &Widget,
    artists_sort: &Widget,
) {
    let s = Arc::clone(state);
    let btn = split_btn.clone();
    spawn_future_local(async move {
        let vm_rx = s.view_mode.subscribe();
        while let Ok(mode) = vm_rx.recv().await {
            btn.set_icon_name(mode.icon_name());
            btn.set_tooltip_text(Some(mode.tooltip()));
        }
    });

    let s2 = Arc::clone(state);
    let albums_sort_btn = albums_sort.clone();
    let artists_sort_btn = artists_sort.clone();
    spawn_future_local(async move {
        let tab_rx = s2.active_tab.subscribe();
        while let Ok(tab) = tab_rx.recv().await {
            let is_albums = tab == Albums;
            albums_sort_btn.set_visible(is_albums);
            artists_sort_btn.set_visible(!is_albums);
        }
    });
}

/// Build a header bar with view toggle and preferences button.
///
/// Creates a horizontal box containing the view split button and a
/// gear icon button to open the preferences dialog.
pub fn build_header_controls(state: &Arc<AppState>, parent: &Window) -> Box {
    let controls = Box::builder().orientation(Horizontal).spacing(6).build();

    let toggle = build_view_toggle(state);
    controls.append(&toggle);

    let prefs_btn = Button::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Preferences")
        .css_classes(["flat"])
        .can_focus(true)
        .build();
    prefs_btn.update_property(&[PropertyLabel("Preferences")]);

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
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::runtime::AppState,
        storage::view_mode::ViewMode::{Column, Grid},
        ui::header::build_view_toggle,
    };

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

    #[test]
    fn build_view_toggle_sets_initial_icon() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let toggle = build_view_toggle(&state);
        ensure!(toggle.icon_name().as_deref() == Some("view-grid-symbolic"));
        Ok(())
    }
}

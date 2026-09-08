//! `HeaderBar` with Albums/Artists tab buttons and view toggle controls.
//!
//! Tab navigation lives in the sibling [`panes`](crate::ui::panes) module: a
//! `ViewSwitcher` sits in the header bar title slot for wide windows and is
//! replaced by a bottom `ViewSwitcherBar` in narrow windows per GNOME HIG.
//!
//! Provides a `SplitButton` to toggle between grid and column layout views
//! with a popover containing zoom controls, sort configuration, and a
//! preferences entry. The popover construction lives in the sibling
//! [`toggle_popover`] module.

use std::sync::Arc;

use {
    libadwaita::{
        SplitButton,
        glib::spawn_future_local,
        gtk::{Widget, Window, accessible::Property::Label as PropertyLabel},
        prelude::{AccessibleExtManual, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::Albums,
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::toggle_popover::build_popover,
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
/// controls (zoom out/in), sort configuration lists (albums/artists),
/// and a preferences entry.
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings
/// * `parent` - Parent window used to present the preferences dialog
///
/// # Returns
///
/// A `SplitButton` with attached popover for view, sort, and preferences
/// controls.
pub fn build_view_toggle(state: &Arc<AppState>, parent: &Window) -> SplitButton {
    let initial_mode = state.storage.get_view_mode();

    let split_btn = SplitButton::builder()
        .icon_name(initial_mode.icon_name())
        .tooltip_text("Toggle View")
        .can_focus(true)
        .build();
    split_btn.update_property(&[PropertyLabel(initial_mode.tooltip())]);

    let (popover, albums_sort, artists_sort) = build_popover(state, parent);
    split_btn.set_popover(Some(&popover));

    let state_clone = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(split_btn.connect_clicked(move |btn| {
            let current_mode = state_clone.storage.get_view_mode();
            let mode = if current_mode == Grid { Column } else { Grid };
            btn.set_icon_name(mode.icon_name());
            btn.set_tooltip_text(Some(mode.tooltip()));
            let sc = Arc::clone(&state_clone);
            state_clone
                .handles
                .lock()
                .retain_task(spawn_future_local(save_view_mode(sc, mode)));
            state_clone.view_mode.send(mode);
        }));

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
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let vm_rx = s.view_mode.subscribe();
            while let Ok(mode) = vm_rx.recv().await {
                btn.set_icon_name(mode.icon_name());
                btn.set_tooltip_text(Some(mode.tooltip()));
            }
        }));

    let s2 = Arc::clone(state);
    let albums_sort_btn = albums_sort.clone();
    let artists_sort_btn = artists_sort.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let tab_rx = s2.active_tab.subscribe();
            while let Ok(tab) = tab_rx.recv().await {
                let is_albums = tab == Albums;
                albums_sort_btn.set_visible(is_albums);
                artists_sort_btn.set_visible(!is_albums);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Context, Result, ensure},
        libadwaita::gtk::{self, Window, test},
        tempfile::tempdir,
        tokio::runtime::Runtime,
    };

    use crate::{
        app::mocks::{build_app_state, fresh_storage},
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
        let dir = tempdir()?;
        let rt = Runtime::new().context("Failed to create tokio runtime")?;
        let storage = rt.block_on(fresh_storage(dir.path()))?;
        let state = Arc::new(build_app_state(storage));
        let window = Window::new();
        let toggle = build_view_toggle(&state, &window);
        ensure!(toggle.icon_name().as_deref() == Some("view-grid-symbolic"));
        Ok(())
    }
}

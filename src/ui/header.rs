//! `HeaderBar` with Albums/Artists tab buttons and view toggle controls.
//!
//! Tab navigation lives in the sibling [`panes`](crate::ui::panes) module: a
//! `ViewSwitcher` sits in the header bar title slot for wide windows and is
//! replaced by a bottom `ViewSwitcherBar` in narrow windows per GNOME HIG.
//!
//! Provides a `SplitButton` to toggle between grid and column layout views
//! with a popover containing zoom controls, sort configuration, and a
//! preferences entry. The popover construction lives in the sibling
//! [`toggle_popover`] module. While the `Signal` tab is active the view switch
//! is hidden and a plain signal menu (copy plus preferences) is shown
//! instead; its construction lives in the sibling [`signal_menu`] module.

use std::sync::Arc;

use {
    libadwaita::{
        SplitButton,
        glib::spawn_future_local,
        gtk::{Widget, Window, accessible::Property::Label},
        prelude::{AccessibleExtManual, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{Albums, Artists},
        view_mode::ViewMode,
    },
    ui::{
        signal_view::{
            signal_menu::{SignalHeaderMenu, build_signal_header_menu},
            signal_tab::SignalTab,
        },
        toggle_popover::build_popover,
    },
};

/// End controls for the main window header bar.
///
/// Holds the library view-switch control plus the signal options menu. Only
/// one is visible at a time: the toggle for Albums/Artists, the menu for
/// `Signal`.
#[derive(Debug, Clone)]
pub struct HeaderEndControls {
    /// View-switch `SplitButton` for the Albums/Artists tabs.
    pub view_toggle: SplitButton,
    /// Plain signal menu for the `Signal` tab.
    pub signal_menu: SignalHeaderMenu,
}

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
    split_btn.update_property(&[Label(initial_mode.tooltip())]);

    let (popover, albums_sort, artists_sort) = build_popover(state, parent);
    split_btn.set_popover(Some(&popover));

    let state_clone = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(split_btn.connect_clicked(move |btn| {
            let current_mode = state_clone.storage.get_view_mode();
            let mode = current_mode.toggle();
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

/// Build the header-bar end controls swapping view toggle and signal menu.
///
/// Shows the view-switch `SplitButton` for Albums/Artists and a plain
/// signal `MenuButton` (copy plus preferences) for `Signal`. Visibility
/// follows the active tab, initialized from storage.
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings.
/// * `parent` - Parent window used to present the preferences dialog.
/// * `tab` - Signal tab owning the retained path summary for the copy action.
///
/// # Returns
///
/// * `HeaderEndControls` - View toggle plus signal menu with tab visibility wiring.
#[must_use]
pub fn build_header_end_controls(
    state: &Arc<AppState>,
    parent: &Window,
    tab: &SignalTab,
) -> HeaderEndControls {
    let view_toggle = build_view_toggle(state, parent);
    let signal_menu = build_signal_header_menu(state, parent, tab);

    let is_signal = state.storage.get_active_tab().is_signal();
    view_toggle.set_visible(!is_signal);
    signal_menu.menu_button().set_visible(is_signal);

    let toggle = view_toggle.clone();
    let menu = signal_menu.menu_button().clone();
    let tab_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let rx = tab_state.active_tab.subscribe();
            while let Ok(tab) = rx.recv().await {
                let signal = tab.is_signal();
                toggle.set_visible(!signal);
                menu.set_visible(signal);
            }
        }));

    HeaderEndControls {
        view_toggle,
        signal_menu,
    }
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
                albums_sort_btn.set_visible(tab == Albums);
                artists_sort_btn.set_visible(tab == Artists);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, Window, test},
    };

    use crate::{
        app::mocks::isolated_app_state,
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
        let state = Arc::new(isolated_app_state()?);
        let window = Window::new();
        let toggle = build_view_toggle(&state, &window);
        ensure!(toggle.icon_name().as_deref() == Some("view-grid-symbolic"));
        Ok(())
    }
}

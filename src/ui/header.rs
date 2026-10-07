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
//! While an album/artist detail page is pushed (tag `detail` in the
//! `NavigationView`) both controls are hidden: zoom and sort are no-ops
//! there since detail covers use fixed sizes.

use std::sync::Arc;

use {
    libadwaita::{
        NavigationView, SplitButton,
        glib::spawn_future_local,
        gtk::{MenuButton, Widget, Window, accessible::Property::Label},
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
        navigation::is_detail_visible,
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
/// `Signal`. Both are hidden while a detail page is pushed, since zoom and
/// sort are no-ops there.
#[derive(Debug, Clone)]
pub struct HeaderEndControls {
    /// View-switch `SplitButton` for the Albums/Artists tabs.
    pub view_toggle: SplitButton,
    /// Plain signal menu for the `Signal` tab.
    pub signal_menu: SignalHeaderMenu,
}

/// Update header end-control visibility for tab and detail state.
///
/// Shows the view-switch `SplitButton` for Albums/Artists, the plain signal
/// `MenuButton` for `Signal`, and neither while a detail page is pushed.
///
/// # Arguments
///
/// * `view_toggle` - View-switch control for the Albums/Artists tabs.
/// * `menu_button` - Plain signal menu button for the `Signal` tab.
/// * `is_signal` - Whether the `Signal` tab is active.
/// * `is_detail` - Whether a detail page is pushed.
fn update_header_visibility(
    view_toggle: &SplitButton,
    menu_button: &MenuButton,
    is_signal: bool,
    is_detail: bool,
) {
    view_toggle.set_visible(!is_signal && !is_detail);
    menu_button.set_visible(is_signal && !is_detail);
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
/// follows the active tab, initialized from storage. Both controls are
/// hidden while a detail page is pushed, since zoom and sort are no-ops
/// there; visibility is restored when the detail page is popped, covering
/// `Back` events as well as built-in `pop_on_escape` pops.
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings.
/// * `parent` - Parent window used to present the preferences dialog.
/// * `tab` - Signal tab owning the retained path summary for the copy action.
/// * `nav_view` - Navigation view checked for pushed detail pages.
///
/// # Returns
///
/// * `HeaderEndControls` - View toggle plus signal menu with tab and detail visibility wiring.
#[must_use]
pub fn build_header_end_controls(
    state: &Arc<AppState>,
    parent: &Window,
    tab: &SignalTab,
    nav_view: &NavigationView,
) -> HeaderEndControls {
    let view_toggle = build_view_toggle(state, parent);
    let signal_menu = build_signal_header_menu(state, parent, tab);

    let is_signal = state.storage.get_active_tab().is_signal();
    let is_detail = is_detail_visible(nav_view);
    update_header_visibility(
        &view_toggle,
        signal_menu.menu_button(),
        is_signal,
        is_detail,
    );

    let toggle = view_toggle.clone();
    let menu = signal_menu.menu_button().clone();
    let tab_state = Arc::clone(state);
    let tab_nav = nav_view.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let rx = tab_state.active_tab.subscribe();
            while let Ok(tab) = rx.recv().await {
                let signal = tab.is_signal();
                let detail = is_detail_visible(&tab_nav);
                update_header_visibility(&toggle, &menu, signal, detail);
            }
        }));

    let detail_toggle = view_toggle.clone();
    let detail_menu = signal_menu.menu_button().clone();
    let detail_state = Arc::clone(state);
    let detail_nav = nav_view.clone();
    state
        .handles
        .lock()
        .retain_signal(nav_view.connect_visible_page_notify(move |_| {
            let signal = detail_state.storage.get_active_tab().is_signal();
            let detail = is_detail_visible(&detail_nav);
            update_header_visibility(&detail_toggle, &detail_menu, signal, detail);
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
        libadwaita::{
            NavigationView,
            gtk::{self, Window, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        app::{
            mocks::isolated_app_state,
            runtime::NavigationEvent::{AlbumDetail, Back},
        },
        storage::view_mode::ViewMode::{Column, Grid},
        ui::{
            header::{build_header_end_controls, build_view_toggle},
            navigation::{handle_navigation_event, tests::add_library_page},
            signal_view::signal_tab_build::build_signal_page,
        },
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

    #[test]
    fn header_toggle_visible_on_library_hidden_on_detail() -> Result<()> {
        let state = Arc::new(isolated_app_state()?);
        let parent = Window::new();
        let signal_tab = build_signal_page();
        let nav_view = NavigationView::new();
        let nav_tx = add_library_page(&nav_view, &state);
        let controls = build_header_end_controls(&state, &parent, &signal_tab, &nav_view);
        ensure!(
            controls.view_toggle.is_visible(),
            "view toggle must be visible on the library page"
        );
        ensure!(
            !controls.signal_menu.menu_button().is_visible(),
            "signal menu must be hidden on the library page"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, AlbumDetail(1));
        ensure!(
            !controls.view_toggle.is_visible(),
            "view toggle (zoom controls) must be hidden on detail pages"
        );
        ensure!(
            !controls.signal_menu.menu_button().is_visible(),
            "signal menu must stay hidden on detail pages"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            controls.view_toggle.is_visible(),
            "view toggle must reappear after popping back to the library"
        );
        Ok(())
    }
}

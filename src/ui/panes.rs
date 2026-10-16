//! Sidebar and content panes.

use std::sync::{Arc, atomic::AtomicBool};

use libadwaita::{
    HeaderBar, NavigationView, OverlaySplitView, ToastOverlay, ToolbarView, ViewStack,
    glib::{Object, object::ObjectExt, spawn_future_local},
    gtk::{Button, ToggleButton, Window, accessible::Property::Label},
    prelude::{AccessibleExtManual, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    ui::{
        detail_nav::handle_navigation_event,
        gallery::narrow_flag::NarrowState,
        header::build_header_end_controls,
        library_stack::{build_library_page, build_library_stack, wire_library_signals},
        pane_sidebar::{build_sidebar, place_sidebar_toggle},
        pane_switcher::{SwitcherGroup, build_switcher_group},
        player::sidebar_toggles::wire_sidebar_toggles,
        popover_zoom::notify_zoom_change,
        signal_view::signal_poll::wire_signal_tab,
        status::StatusBar,
        switching::wire_tab_tracking,
    },
};

/// Fan out narrow-window changes as zoom notifications.
///
/// Reuses the zoom channels so ready grids resize in place and unready ones
/// rebuild from cache once shown.
fn wire_narrow_fit(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) {
    let rx = narrow_state.subscribe();
    let fit_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            while rx.recv().await.is_ok() {
                notify_zoom_change(&fit_state);
            }
        }));
}

/// Build the content pane with library views and controls.
fn build_content_pane(
    state: &Arc<AppState>,
    toggle_button: &ToggleButton,
    narrow_state: &Arc<NarrowState>,
    parent: &Window,
) -> (ToolbarView, ViewStack, NavigationView, SwitcherGroup) {
    let content_toolbar = ToolbarView::builder()
        .css_classes(["content-toolbar"])
        .tooltip_text("Library toolbar")
        .build();
    let content_header = HeaderBar::builder()
        .css_classes(["content-header"])
        .tooltip_text("Library header")
        .build();
    let (stack, album_stack, artist_stack, signal_tab) = build_library_stack(state, narrow_state);
    wire_narrow_fit(state, narrow_state);
    wire_library_signals(
        state,
        &stack,
        album_stack,
        artist_stack,
        Arc::clone(narrow_state),
    );
    wire_signal_tab(state, &signal_tab);
    let (switcher, switcher_bar) = build_switcher_group(&stack);
    content_header.set_title_widget(Some(&switcher));
    let nav_view = NavigationView::builder()
        .css_classes(["library-nav"])
        .tooltip_text("Library navigation")
        .build();
    nav_view.update_property(&[Label("Library navigation")]);
    nav_view.set_pop_on_escape(true);
    nav_view.add(&build_library_page(&stack));
    let controls = build_header_end_controls(state, parent, &signal_tab, &nav_view);
    content_header.pack_end(&controls.view_toggle);
    content_header.pack_end(controls.signal_menu.menu_button());
    content_header.pack_start(toggle_button);
    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&nav_view));
    content_toolbar.add_bottom_bar(&switcher_bar);
    let status_bar = StatusBar::new(state);
    content_toolbar.add_bottom_bar(status_bar.widget());
    let switchers = SwitcherGroup {
        header: content_header,
        switcher,
        bar: switcher_bar,
    };
    (content_toolbar, stack, nav_view, switchers)
}

/// Build split-view content with sidebar and content panes.
///
/// Returns the toast overlay, split view, toggle buttons, close button,
/// switchers, and nav view for `build_window` (the nav view lets key
/// controllers ignore zoom while a detail page is pushed). The overlay uses
/// the generic object builder since libadwaita 0.9.2 generates no
/// `ToastOverlay` builder, like `AboutWindow`.
///
/// # Arguments
///
/// * `state` - Application state owning storage and signal handles.
/// * `narrow_state` - Shared narrow-mode flag for adaptive wiring.
/// * `parent` - Parent window for dialogs spawned from the content pane.
/// * `sidebar_intent` - Last sidebar intent, seeded from settings and updated by toggles and
///   playback events.
pub fn build_content(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
    parent: &Window,
    sidebar_intent: &Arc<AtomicBool>,
) -> (
    ToastOverlay,
    OverlaySplitView,
    ToggleButton,
    ToggleButton,
    Button,
    SwitcherGroup,
    NavigationView,
) {
    let toast_overlay = Object::builder::<ToastOverlay>()
        .property("css-classes", vec![String::from("toast-overlay")])
        .property("tooltip-text", "Notifications")
        .property("can-focus", true)
        .build();
    toast_overlay.update_property(&[Label("Notification overlay")]);
    let sidebar_visible = state.storage.get_sidebar_visible();

    let back_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .css_classes(["flat", "toggle"])
        .tooltip_text("Hide player panel")
        .active(sidebar_visible)
        .build();
    back_button.update_property(&[Label("Hide player panel")]);
    back_button.set_visible(sidebar_visible);

    let (sidebar_toolbar, close_button, sidebar_header) = build_sidebar(state, &back_button);

    let toggle_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Toggle player panel (Ctrl+B)")
        .active(sidebar_visible)
        .css_classes(["flat", "toggle"])
        .build();
    toggle_button.update_property(&[Label("Toggle player panel")]);
    toggle_button.set_visible(!sidebar_visible);

    let (content_toolbar, stack, nav_view, switchers) =
        build_content_pane(state, &toggle_button, narrow_state, parent);

    wire_tab_tracking(state, &stack, &nav_view);

    let split_view = OverlaySplitView::builder()
        .sidebar(&sidebar_toolbar)
        .content(&content_toolbar)
        .min_sidebar_width(320.0)
        .max_sidebar_width(400.0)
        .show_sidebar(sidebar_visible)
        .pin_sidebar(true)
        .css_classes(["main-split"])
        .tooltip_text("Player panel — toggle with button in header")
        .build();
    split_view.update_property(&[Label("Main player panel with sidebar and content area")]);

    {
        let header = sidebar_header.clone();
        let back = back_button.clone();
        state
            .handles
            .lock()
            .retain_signal(
                split_view.connect_notify_local(Some("collapsed"), move |sv, _| {
                    place_sidebar_toggle(&header, &back, sv.is_collapsed());
                }),
            );
        place_sidebar_toggle(&sidebar_header, &back_button, split_view.is_collapsed());
    }

    wire_sidebar_toggles(
        state,
        &split_view,
        &toggle_button,
        &back_button,
        sidebar_intent,
    );

    toast_overlay.set_child(Some(&split_view));

    let nav_tx = state.navigation_tx.clone();
    let nav_state = Arc::clone(state);
    let nav_view_loop = nav_view.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let rx = nav_state.navigation_rx.clone();
            while let Ok(event) = rx.recv().await {
                handle_navigation_event(&nav_state, &nav_view_loop, &nav_tx, event);
            }
        }));

    (
        toast_overlay,
        split_view,
        toggle_button,
        back_button,
        close_button,
        switchers,
        nav_view,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, Window, accessible::Property::Label, test},
            prelude::{AccessibleExtManual, WidgetExt},
        },
    };

    use crate::{
        app::{mocks::pump_in_test_runtime, runtime::AppState},
        ui::{gallery::narrow_flag::NarrowState, panes::build_content},
    };

    #[test]
    fn content_shell_stays_in_focus_chain() -> Result<()> {
        pump_in_test_runtime(|| {
            let state = Arc::new(AppState::mock()?);
            let narrow_state = NarrowState::new_shared();
            let parent = Window::builder()
                .css_classes(["test-parent", "background"])
                .tooltip_text("Test parent window")
                .build();
            parent.update_property(&[Label("Test parent window")]);
            let intent = Arc::new(AtomicBool::new(true));
            let (overlay, split, _, _, _, _, nav_view) =
                build_content(&state, &narrow_state, &parent, &intent);
            ensure!(
                overlay.can_focus(),
                "toast overlay must stay focusable so keyboard focus (and Escape) reaches the \
                 library"
            );
            ensure!(
                split.can_focus(),
                "split view must stay focusable so keyboard focus (and Escape) reaches the library"
            );
            ensure!(
                nav_view.can_focus(),
                "navigation view must stay focusable so keyboard focus (and Escape-to-back) \
                 reaches pushed detail pages"
            );
            Ok(())
        })?
    }
}

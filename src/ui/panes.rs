//! Sidebar and content panes.

use std::sync::{Arc, atomic::AtomicBool};

use libadwaita::{
    HeaderBar, NavigationView, OverlaySplitView, ToastOverlay, ToolbarView, ViewStack,
    ViewSwitcher, ViewSwitcherBar,
    ViewSwitcherPolicy::Wide,
    WindowTitle,
    glib::{object::ObjectExt, spawn_future_local},
    gtk::{Button, Stack, ToggleButton, Window, accessible::Property::Label},
    prelude::{AccessibleExtManual, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    storage::active_tab::ActiveTab::{Albums, Artists, Signal},
    ui::{
        gallery::{
            album_grid::build_album_grid, artist_grid::build_artist_grid, narrow_flag::NarrowState,
        },
        header::build_header_end_controls,
        navigation::{build_library_page, handle_navigation_event},
        pane_modes::switch_mode_for_active_tab,
        player::{sidebar::build_player_content, sidebar_toggles::wire_sidebar_toggles},
        signal_view::{
            signal_poll::wire_signal_tab, signal_tab::SignalTab,
            signal_tab_build::build_signal_page,
        },
        status::StatusBar,
        switching::{handle_tab_switch, wire_tab_tracking},
        toggle_popover::notify_zoom_change,
    },
};

/// Library view switcher widgets shared with the window shell for adaptive
/// narrow-mode wiring.
#[derive(Debug)]
pub struct SwitcherGroup {
    /// Header bar holding the wide-mode `ViewSwitcher` in its title slot.
    pub header: HeaderBar,
    /// Wide-mode switcher shown in the header bar title slot.
    pub switcher: ViewSwitcher,
    /// Narrow-mode `ViewSwitcherBar` shown at the bottom of the content pane.
    pub bar: ViewSwitcherBar,
}

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
fn build_sidebar(
    state: &Arc<AppState>,
    back_button: &ToggleButton,
) -> (ToolbarView, Button, HeaderBar) {
    let sidebar_toolbar = ToolbarView::new();

    let close_button = Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close application")
        .css_classes(["flat"])
        .can_focus(true)
        .build();
    close_button.update_property(&[Label("Close application")]);

    let sidebar_header = HeaderBar::new();
    sidebar_header.set_title_widget(Some(&WindowTitle::new("Now Playing", "")));
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
fn place_sidebar_toggle(header: &HeaderBar, back_button: &ToggleButton, close_visible: bool) {
    header.remove(back_button);
    if close_visible {
        header.pack_start(back_button);
    } else {
        header.pack_end(back_button);
    }
}

/// Build the library `ViewStack` with album, artist, and signal pages.
fn build_library_stack(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
) -> (ViewStack, Stack, Stack, SignalTab) {
    let stack = ViewStack::new();
    stack.set_vexpand(true);
    let album_grid = build_album_grid(state, narrow_state);
    drop(stack.add_titled_with_icon(
        &album_grid.mode_stack,
        Some("albums"),
        "Albums",
        "view-grid-symbolic",
    ));
    let artist_grid = build_artist_grid(state, narrow_state);
    drop(stack.add_titled_with_icon(
        &artist_grid.mode_stack,
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    ));
    let signal_tab = build_signal_page();
    drop(stack.add_titled_with_icon(
        signal_tab.widget(),
        Some("signal"),
        "Signal",
        "audio-x-generic-symbolic",
    ));
    match state.storage.get_active_tab() {
        Artists => stack.set_visible_child_name("artists"),
        Signal => stack.set_visible_child_name("signal"),
        Albums => {}
    }
    (
        stack,
        album_grid.mode_stack,
        artist_grid.mode_stack,
        signal_tab,
    )
}

/// Wire tab and view-mode signals for the library stack.
fn wire_library_signals(
    state: &Arc<AppState>,
    stack: &ViewStack,
    album_stack: Stack,
    artist_stack: Stack,
    narrow_state: Arc<NarrowState>,
) {
    let tab_rx = state.active_tab.subscribe();
    let s1 = stack.clone();
    let st1 = Arc::clone(state);
    let a1 = album_stack.clone();
    let r1 = artist_stack.clone();
    let n1 = Arc::clone(&narrow_state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            while let Ok(tab) = tab_rx.recv().await {
                handle_tab_switch(&s1, &st1, tab, &a1, &r1, &n1);
            }
        }));
    let s2 = Arc::clone(state);
    let a2 = album_stack;
    let r2 = artist_stack;
    let n2 = narrow_state;
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let rx = s2.view_mode.subscribe();
            while let Ok(m) = rx.recv().await {
                switch_mode_for_active_tab(&s2, m, &a2, &r2, &n2);
            }
        }));
}

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
    let content_toolbar = ToolbarView::new();
    let content_header = HeaderBar::new();
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
    let switcher = ViewSwitcher::builder()
        .policy(Wide)
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums, Artists, and Signal views (Ctrl+Tab)")
        .build();
    switcher.update_property(&[Label("Switch between Albums, Artists, and Signal views")]);
    content_header.set_title_widget(Some(&switcher));
    let nav_view = NavigationView::new();
    nav_view.set_pop_on_escape(true);
    nav_view.add(&build_library_page(&stack));
    let controls = build_header_end_controls(state, parent, &signal_tab, &nav_view);
    content_header.pack_end(&controls.view_toggle);
    content_header.pack_end(controls.signal_menu.menu_button());
    content_header.pack_start(toggle_button);
    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&nav_view));
    let switcher_bar = ViewSwitcherBar::builder()
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums, Artists, and Signal views (Ctrl+Tab)")
        .build();
    switcher_bar.update_property(&[Label("Switch between Albums, Artists, and Signal views")]);
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
/// controllers ignore zoom while a detail page is pushed).
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
    let toast_overlay = ToastOverlay::new();
    let sidebar_visible = state.storage.get_sidebar_visible();

    let back_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Hide player panel")
        .active(sidebar_visible)
        .can_focus(true)
        .build();
    back_button.update_property(&[Label("Hide player panel")]);
    back_button.set_visible(sidebar_visible);

    let (sidebar_toolbar, close_button, sidebar_header) = build_sidebar(state, &back_button);

    let toggle_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Toggle player panel (Ctrl+B)")
        .active(sidebar_visible)
        .css_classes(["flat"])
        .can_focus(true)
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
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            HeaderBar, ViewStack, ViewSwitcher, ViewSwitcherBar,
            gtk::{self, test},
        },
    };

    use crate::ui::panes::SwitcherGroup;

    #[test]
    fn switcher_group_fields_are_accessible() -> Result<()> {
        let stack = ViewStack::new();
        let group = SwitcherGroup {
            header: HeaderBar::new(),
            switcher: ViewSwitcher::builder().stack(&stack).build(),
            bar: ViewSwitcherBar::builder().stack(&stack).build(),
        };
        ensure!(
            group.switcher.stack().is_some(),
            "switcher must be bound to a stack"
        );
        ensure!(
            group.bar.stack().is_some(),
            "switcher bar must be bound to a stack"
        );
        Ok(())
    }
}

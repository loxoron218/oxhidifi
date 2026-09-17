//! Sidebar and content panes.

use std::sync::Arc;

use libadwaita::{
    HeaderBar, NavigationPage, NavigationView, OverlaySplitView, ToastOverlay, ToolbarView,
    ViewStack, ViewSwitcher, ViewSwitcherBar,
    ViewSwitcherPolicy::Wide,
    WindowTitle,
    glib::spawn_future_local,
    gtk::{Button, Stack, ToggleButton, Window, accessible::Property::Label},
    prelude::{AccessibleExtManual, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{Albums, Artists},
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::{
        gallery::{
            album_grid::{build_album_grid, lazy_build_album_mode},
            artist_grid::{build_artist_grid, lazy_build_artist_mode},
            narrow_flag::NarrowState,
        },
        header::build_view_toggle,
        navigation::handle_navigation_event,
        player::{sidebar::build_player_content, wire_sidebar_toggles},
        status::StatusBar,
        switching::{handle_tab_switch, wire_tab_tracking},
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
/// Returns the `ToolbarView` and a close button that is shown in
/// collapsed mode (see `build_window`).
fn build_sidebar(state: &Arc<AppState>, back_button: &ToggleButton) -> (ToolbarView, Button) {
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
    sidebar_header.pack_start(back_button);
    sidebar_header.pack_end(&close_button);

    sidebar_toolbar.add_top_bar(&sidebar_header);

    let player_content = build_player_content(state);
    sidebar_toolbar.set_content(Some(&player_content));

    (sidebar_toolbar, close_button)
}

/// Build the library `ViewStack` with album and artist pages.
fn build_library_stack(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
) -> (ViewStack, Stack, Stack) {
    let stack = ViewStack::new();
    stack.set_vexpand(true);
    let album_grid = build_album_grid(state, narrow_state);
    let ac = stack.add_titled_with_icon(
        &album_grid.mode_stack,
        Some("albums"),
        "Albums",
        "view-grid-symbolic",
    );
    ac.set_icon_name(Some("view-grid-symbolic"));
    let artist_grid = build_artist_grid(state, narrow_state);
    let ar = stack.add_titled_with_icon(
        &artist_grid.mode_stack,
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    );
    ar.set_icon_name(Some("avatar-default-symbolic"));
    if state.storage.get_active_tab() == Artists {
        stack.set_visible_child_name("artists");
    }
    (stack, album_grid.mode_stack, artist_grid.mode_stack)
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

/// Build the content pane with library views and controls.
fn build_content_pane(
    state: &Arc<AppState>,
    toggle_button: &ToggleButton,
    narrow_state: &Arc<NarrowState>,
    parent: &Window,
) -> (ToolbarView, ViewStack, NavigationView, SwitcherGroup) {
    let content_toolbar = ToolbarView::new();
    let content_header = HeaderBar::new();
    let (stack, album_stack, artist_stack) = build_library_stack(state, narrow_state);
    wire_library_signals(
        state,
        &stack,
        album_stack,
        artist_stack,
        Arc::clone(narrow_state),
    );
    let switcher = ViewSwitcher::builder()
        .policy(Wide)
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();
    switcher.update_property(&[Label("Switch between Albums and Artists views")]);
    content_header.set_title_widget(Some(&switcher));
    let toggle = build_view_toggle(state, parent);
    content_header.pack_end(&toggle);
    content_header.pack_start(toggle_button);
    content_toolbar.add_top_bar(&content_header);
    let nav_view = NavigationView::new();
    nav_view.set_pop_on_escape(true);
    let library_page = NavigationPage::builder()
        .child(&stack)
        .title("Library")
        .tag("library")
        .build();
    nav_view.add(&library_page);
    content_toolbar.set_content(Some(&nav_view));
    let switcher_bar = ViewSwitcherBar::builder()
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();
    switcher_bar.update_property(&[Label("Switch between Albums and Artists views")]);
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
/// Returns `(ToastOverlay, OverlaySplitView, toggle_button, back_button,
/// close_button, switchers)` for `build_window`.
pub fn build_content(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
    parent: &Window,
) -> (
    ToastOverlay,
    OverlaySplitView,
    ToggleButton,
    ToggleButton,
    Button,
    SwitcherGroup,
) {
    let toast_overlay = ToastOverlay::new();

    let back_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Hide player panel")
        .active(true)
        .can_focus(true)
        .build();
    back_button.update_property(&[Label("Hide player panel")]);
    back_button.set_visible(false);

    let (sidebar_toolbar, close_button) = build_sidebar(state, &back_button);

    let toggle_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Toggle player panel")
        .active(false)
        .css_classes(["flat"])
        .can_focus(true)
        .build();
    toggle_button.update_property(&[Label("Toggle player panel")]);

    let (content_toolbar, stack, nav_view, switchers) =
        build_content_pane(state, &toggle_button, narrow_state, parent);

    wire_tab_tracking(state, &stack, &nav_view);

    let split_view = OverlaySplitView::builder()
        .sidebar(&sidebar_toolbar)
        .content(&content_toolbar)
        .min_sidebar_width(320.0)
        .max_sidebar_width(400.0)
        .show_sidebar(false)
        .pin_sidebar(true)
        .tooltip_text("Player panel — toggle with button in header")
        .build();
    split_view.update_property(&[Label("Main player panel with sidebar and content area")]);

    wire_sidebar_toggles(state, &split_view, &toggle_button, &back_button);

    toast_overlay.set_child(Some(&split_view));

    let nav_tx = state.navigation_tx.clone();
    let nav_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let rx = nav_state.navigation_rx.clone();
            while let Ok(event) = rx.recv().await {
                handle_navigation_event(&nav_state, &nav_view, &nav_tx, event);
            }
        }));

    (
        toast_overlay,
        split_view,
        toggle_button,
        back_button,
        close_button,
        switchers,
    )
}

/// Switch the currently active tab's mode stack to `mode`, building the
/// view lazily if it doesn't exist yet.
///
/// Hidden tabs are skipped — they reconcile their mode when activated via
/// [`handle_tab_switch`], so toggling modes never builds a view the user
/// is not looking at.
fn switch_mode_for_active_tab(
    state: &Arc<AppState>,
    mode: ViewMode,
    album_stack: &Stack,
    artist_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    let (stack, name) = match state.active_tab.borrow() {
        Albums => (album_stack, "albums"),
        Artists => (artist_stack, "artists"),
    };
    switch_mode_for_stack(state, name, stack, narrow_state, mode);
}

/// Return the mode‑stack for a given tab name, or `None` if unknown.
/// Switch the given tab's mode‑stack to `mode`, building the view
/// lazily if it doesn't exist yet.
fn switch_mode_for_stack(
    state: &Arc<AppState>,
    tab: &str,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    mode: ViewMode,
) {
    let child = match mode {
        Grid => "grid",
        Column => "column",
    };
    if stack.child_by_name(child).is_none() {
        match tab {
            "albums" => lazy_build_album_mode(state, stack, narrow_state, mode),
            "artists" => lazy_build_artist_mode(state, stack, mode),
            _ => {}
        }
    }
    stack.set_visible_child_name(child);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            HeaderBar, OverlaySplitView, ToastOverlay, ViewStack, ViewSwitcher, ViewSwitcherBar,
            gtk::{self, Button, ToggleButton, Window, test},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::{
            gallery::narrow_flag::NarrowState,
            panes::{SwitcherGroup, build_content},
        },
    };

    #[test]
    fn build_content_signature_shape() {
        fn assert_shape<
            F: Fn(
                &Arc<AppState>,
                &Arc<NarrowState>,
                &Window,
            ) -> (
                ToastOverlay,
                OverlaySplitView,
                ToggleButton,
                ToggleButton,
                Button,
                SwitcherGroup,
            ),
        >(
            _: F,
        ) {
        }
        assert_shape(build_content);
    }

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

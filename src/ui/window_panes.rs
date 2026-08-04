//! Sidebar and content panes for the main split view.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use {
    libadwaita::{
        HeaderBar, OverlaySplitView, ToastOverlay, ToolbarView, ViewStack, ViewSwitcher,
        ViewSwitcherBar,
        ViewSwitcherPolicy::Wide,
        WindowTitle,
        glib::{
            object::{Cast, ObjectExt},
            spawn_future_local,
        },
        gtk::{
            Button, Stack, ToggleButton, Widget, Window, accessible::Property::Label,
            prelude::ToggleButtonExt,
        },
        prelude::{AccessibleExtManual, WidgetExt},
    },
    tracing::{error, info},
};

use crate::{
    app::{AppState, NavigationEvent::Back},
    storage::settings::{
        ActiveTab::{Albums, Artists},
        ViewMode::{self, Column, Grid},
    },
    ui::{
        header::build_header_controls,
        library::{
            albums::{build_album_grid, lazy_build_album_mode},
            artists::{build_artist_grid, lazy_build_artist_mode},
            narrow_state::NarrowState,
        },
        player::panel::build_player_content,
        status::StatusBar,
        window_navigation::{handle_navigation_event, persist_active_tab},
    },
};

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

/// Build the content pane with library views and controls.
fn build_content_pane(
    state: &Arc<AppState>,
    toggle_button: &ToggleButton,
    narrow_state: &Arc<NarrowState>,
    parent: &Window,
) -> (ToolbarView, ViewStack, Stack, Widget) {
    let content_toolbar = ToolbarView::new();

    let content_header = HeaderBar::new();

    let stack = ViewStack::new();
    stack.set_vexpand(true);

    let album_grid = build_album_grid(state, narrow_state);
    let albums_child = stack.add_titled_with_icon(
        &album_grid.mode_stack,
        Some("albums"),
        "Albums",
        "view-grid-symbolic",
    );
    albums_child.set_icon_name(Some("view-grid-symbolic"));

    let artist_grid = build_artist_grid(state, narrow_state);
    let artists_child = stack.add_titled_with_icon(
        &artist_grid.mode_stack,
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    );
    artists_child.set_icon_name(Some("avatar-default-symbolic"));

    match state.storage.get_active_tab() {
        Artists => stack.set_visible_child_name("artists"),
        Albums => {}
    }

    let mut tab_rx = state.active_tab_tx.subscribe();
    let active_tab_stack = stack.clone();
    spawn_future_local(async move {
        while tab_rx.changed().await.is_ok() {
            let tab = *tab_rx.borrow();
            active_tab_stack.set_visible_child_name(match tab {
                Albums => "albums",
                Artists => "artists",
            });
        }
    });

    let vm_state = Arc::clone(state);
    let vm_album_stack = album_grid.mode_stack;
    let vm_artist_stack = artist_grid.mode_stack;
    let vm_nm = Arc::clone(narrow_state);
    spawn_future_local(async move {
        let mut rx = vm_state.view_mode_tx.subscribe();
        while rx.changed().await.is_ok() {
            let mode = *rx.borrow();
            switch_mode_for_stack(&vm_state, "albums", &vm_album_stack, &vm_nm, mode).await;
            switch_mode_for_stack(&vm_state, "artists", &vm_artist_stack, &vm_nm, mode).await;
        }
    });

    let switcher = ViewSwitcher::builder()
        .policy(Wide)
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();
    switcher.update_property(&[Label("Switch between Albums and Artists views")]);
    content_header.set_title_widget(Some(&switcher));

    let controls = build_header_controls(state, parent);
    content_header.pack_end(&controls);
    content_header.pack_start(toggle_button);

    content_toolbar.add_top_bar(&content_header);

    let content_area = Stack::new();
    content_area.set_vexpand(true);
    content_area.set_hexpand(true);
    content_area.add_named(&stack, Some("library"));
    content_area.set_visible_child(&stack);
    content_toolbar.set_content(Some(&content_area));

    let orig_stack = stack.clone().upcast::<Widget>();

    let switcher_bar = ViewSwitcherBar::builder()
        .stack(&stack)
        .can_focus(true)
        .tooltip_text("Switch between Albums and Artists views")
        .build();
    switcher_bar.update_property(&[Label("Switch between Albums and Artists views")]);
    content_toolbar.add_bottom_bar(&switcher_bar);

    let status_bar = StatusBar::new(state);
    content_toolbar.add_bottom_bar(status_bar.widget());

    (content_toolbar, stack, content_area, orig_stack)
}

/// Build the split-view content with sidebar and content panes.
///
/// Each pane has its own `ToolbarView` and `HeaderBar`. The sidebar
/// contains the player panel with a back button and "Now Playing"
/// title. The content pane contains the library view switcher and
/// stack. Bottom bars (view switcher and status) are attached to the
/// content pane.
///
/// Returns the `(ToastOverlay, OverlaySplitView, toggle_button, back_button)` for
/// event wiring in `build_window`.
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

    let (content_toolbar, stack, content_area, orig_stack) =
        build_content_pane(state, &toggle_button, narrow_state, parent);

    let nav_tx = state.navigation_tx.clone();

    let tab_nav_tx = nav_tx.clone();
    let tab_content_area = content_area.clone();
    let tab_orig = orig_stack.clone();
    let tab_stack = stack.clone();
    let tab_storage = Arc::clone(&state.storage);
    let tab_active_tab_tx = state.active_tab_tx.clone();
    stack.connect_visible_child_notify(move |_| {
        if let Some(child) = tab_content_area.visible_child()
            && child == tab_orig
            && let Some(name) = tab_stack.visible_child_name()
        {
            info!(tab_name = name.as_str(), "Tab switched",);
            persist_active_tab(&tab_storage, &tab_active_tab_tx, name.as_str());
        }
        let visible = tab_content_area.visible_child();
        let is_on_detail = visible.as_ref().is_none_or(|child| *child != tab_orig);
        if is_on_detail && let Err(err) = tab_nav_tx.try_send(Back) {
            error!(error = %err, "Failed to send Back navigation event");
        }
    });

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

    let user_wants_sidebar = Arc::new(AtomicBool::new(false));

    let sv = split_view.clone();
    let intended = Arc::clone(&user_wants_sidebar);
    toggle_button.connect_toggled(move |btn| {
        intended.store(btn.is_active(), Relaxed);
        if sv.shows_sidebar() != btn.is_active() {
            sv.set_show_sidebar(btn.is_active());
        }
    });

    let sv_back = split_view.clone();
    let intended_back = Arc::clone(&user_wants_sidebar);
    back_button.connect_toggled(move |btn| {
        intended_back.store(btn.is_active(), Relaxed);
        if sv_back.shows_sidebar() != btn.is_active() {
            sv_back.set_show_sidebar(btn.is_active());
        }
    });

    let sv_collapse = split_view.clone();
    let intended_collapse = Arc::clone(&user_wants_sidebar);
    split_view.connect_notify(Some("collapsed"), move |sv, _| {
        if sv.is_collapsed() {
            return;
        }
        let wants = intended_collapse.load(Relaxed);
        if sv.shows_sidebar() != wants {
            sv.set_show_sidebar(wants);
        }
    });

    toast_overlay.set_child(Some(&split_view));

    let nav_state = Arc::clone(state);
    let nav_content_area = content_area;
    spawn_future_local(async move {
        let rx = nav_state.navigation_rx.clone();
        while let Ok(event) = rx.recv().await {
            handle_navigation_event(&nav_state, &nav_content_area, &nav_tx, &orig_stack, event);
        }
    });

    drop(sv_collapse);
    (
        toast_overlay,
        split_view,
        toggle_button,
        back_button,
        close_button,
    )
}

/// Return the mode‑stack for a given tab name, or `None` if unknown.
/// Switch the given tab's mode‑stack to `mode`, building the view
/// lazily if it doesn't exist yet.
async fn switch_mode_for_stack(
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
            "albums" => lazy_build_album_mode(state, stack, narrow_state, mode).await,
            "artists" => lazy_build_artist_mode(state, stack, mode).await,
            _ => {}
        }
    }
    stack.set_visible_child_name(child);
}

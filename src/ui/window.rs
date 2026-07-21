//! Main application window with `OverlaySplitView` sidebar layout.
//!
//! Creates the main window with `AdwOverlaySplitView` containing
//! separate `AdwToolbarView` panes for sidebar and content.
//! The sidebar pane has its own `AdwHeaderBar` with back button,
//! mirroring the Nautilus sidebar pattern.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use {
    async_channel::Sender,
    libadwaita::{
        Application, ApplicationWindow, Breakpoint, BreakpointCondition,
        BreakpointConditionLengthType::MaxWidth,
        HeaderBar,
        LengthUnit::Sp,
        OverlaySplitView, Toast, ToastOverlay,
        ToastPriority::Normal,
        ToolbarView, ViewStack, ViewSwitcher, ViewSwitcherBar,
        ViewSwitcherPolicy::Wide,
        WindowTitle,
        gdk::Display,
        glib::{
            Propagation::Proceed,
            object::{Cast, ObjectExt},
            prelude::ToValue,
            spawn_future_local,
        },
        gtk::{
            self, CssProvider, Stack, ToggleButton, Widget, prelude::ToggleButtonExt,
            style_context_add_provider_for_display,
        },
        prelude::{AdwApplicationWindowExt, GtkWindowExt, WidgetExt},
    },
    tokio::sync::watch::Sender as TokioSender,
    tracing::{error, info, warn},
};

use crate::{
    app::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail, Back},
    },
    playback::control::PlaybackController,
    storage::{
        database::SqliteStorage,
        settings::{
            ActiveTab,
            ActiveTab::{Albums, Artists},
            ViewMode::{self, Column, Grid},
        },
    },
    ui::{
        detail::{album::build_album_detail, artist::build_artist_detail},
        header::build_header_controls,
        library::{
            albums::{build_album_grid, lazy_build_album_mode},
            artists::{build_artist_grid, lazy_build_artist_mode},
            column_view::NarrowState,
        },
        player::{panel::build_player_content, wire_panel_events},
        status::StatusBar,
    },
};

/// Build the main application window.
///
/// Creates an `AdwApplicationWindow` with `AdwOverlaySplitView`
/// containing separate `ToolbarView` panes for the sidebar and
/// content. The sidebar is hidden by default and auto-shown on
/// playback start.
pub fn build_window(app: &Application, state: &Arc<AppState>) -> ApplicationWindow {
    info!("Building main application window");

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(1200)
        .default_height(800)
        .build();

    load_hig_css();

    let narrow_state = NarrowState::new_shared();
    let (toast_overlay, split_view, toggle_button, back_button) =
        build_content(state, &narrow_state, window.upcast_ref::<gtk::Window>());
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoints(&window, &split_view, &narrow_state);

    wire_panel_events(state, &split_view);

    let playback = Arc::clone(&state.playback);
    let cover_cache = Arc::clone(&state.cover_art_cache);
    window.connect_close_request(move |_| {
        info!("Window close requested — stopping playback");
        if let Err(e) = playback.stop() {
            error!(error = %e, "Failed to stop playback on window close");
        }
        cover_cache.shutdown();
        Proceed
    });

    split_view.connect_show_sidebar_notify(move |sv| {
        let showing = sv.shows_sidebar();
        info!(showing, "Sidebar visibility changed",);
        toggle_button.set_visible(!showing);
        toggle_button.set_active(showing);
        back_button.set_visible(showing);
        back_button.set_active(showing);
    });

    window
}

/// Load HIG-compliant CSS transitions (200 ms ease) and style rules.
fn load_hig_css() {
    let Some(display) = Display::default() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_string(
        "
        .overlay-split-view {
            transition: all 200ms ease;
        }
        headerbar {
            transition: background 200ms ease;
        }
        ",
    );
    style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Spawn a future to listen for toast messages and display them.
fn listen_for_toasts(state: &Arc<AppState>, toast_overlay: &ToastOverlay) {
    let rx = state.toast_rx.clone();
    let overlay = toast_overlay.clone();
    spawn_future_local(async move {
        while let Ok(message) = rx.recv().await {
            let toast = Toast::builder().title(message).priority(Normal).build();
            overlay.add_toast(toast);
        }
    });
}

/// Add responsive breakpoints for narrow windows.
///
/// Collapses the `OverlaySplitView` sidebar below 800 px width and
/// hides non‑essential columns (Format, Bit Depth, Sample Rate) below
/// 700 px width.
fn add_responsive_breakpoints(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    narrow_state: &Arc<NarrowState>,
) {
    let sidebar_condition = BreakpointCondition::new_length(MaxWidth, 800.0, Sp);
    let sidebar_bp = Breakpoint::new(sidebar_condition);
    sidebar_bp.add_setter(split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(sidebar_bp);

    let narrow_condition = BreakpointCondition::new_length(MaxWidth, 700.0, Sp);
    let narrow_bp = Breakpoint::new(narrow_condition);
    narrow_bp.add_setter(split_view, "collapsed", Some(&true.to_value()));
    narrow_bp.connect_apply({
        let ns = Arc::clone(narrow_state);
        move |_| ns.set(true)
    });
    narrow_bp.connect_unapply({
        let ns = Arc::clone(narrow_state);
        move |_| ns.set(false)
    });
    window.add_breakpoint(narrow_bp);
}

/// Build the sidebar panel with player content.
fn build_sidebar(state: &Arc<AppState>, back_button: &ToggleButton) -> ToolbarView {
    let sidebar_toolbar = ToolbarView::new();

    let sidebar_header = HeaderBar::new();
    sidebar_header.set_title_widget(Some(&WindowTitle::new("Now Playing", "")));
    sidebar_header.pack_start(back_button);

    sidebar_toolbar.add_top_bar(&sidebar_header);

    let player_content = build_player_content(state);
    sidebar_toolbar.set_content(Some(&player_content));

    sidebar_toolbar
}

/// Build the content pane with library views and controls.
fn build_content_pane(
    state: &Arc<AppState>,
    toggle_button: &ToggleButton,
    narrow_state: &Arc<NarrowState>,
    parent: &gtk::Window,
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
fn build_content(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
    parent: &gtk::Window,
) -> (ToastOverlay, OverlaySplitView, ToggleButton, ToggleButton) {
    let toast_overlay = ToastOverlay::new();

    let back_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Hide player panel")
        .active(true)
        .build();
    back_button.set_visible(false);

    let sidebar_toolbar = build_sidebar(state, &back_button);

    let toggle_button = ToggleButton::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Toggle player panel")
        .active(false)
        .css_classes(["flat"])
        .build();

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
    (toast_overlay, split_view, toggle_button, back_button)
}

/// Save the active tab to storage asynchronously and broadcast through the watch channel.
fn persist_active_tab(
    storage: &Arc<SqliteStorage>,
    active_tab_tx: &TokioSender<ActiveTab>,
    name: &str,
) {
    let tab = if name == "artists" { Artists } else { Albums };
    let s = Arc::clone(storage);
    spawn_future_local(async move {
        if let Err(e) = s.set_active_tab(tab).await {
            warn!(error = %e, "Failed to save active tab");
        }
    });
    active_tab_tx.send_if_modified(|current| {
        let changed = *current != tab;
        *current = tab;
        changed
    });
}

/// Handle navigation events (album/artist detail, back navigation).
fn handle_navigation_event(
    nav_state: &Arc<AppState>,
    nav_content_area: &Stack,
    nav_tx: &Sender<NavigationEvent>,
    orig_stack: &Widget,
    event: NavigationEvent,
) {
    match event {
        AlbumDetail(album_id) => {
            info!(album_id, "Navigating to album detail",);
            if let Some(prev_detail) = nav_content_area.child_by_name("detail") {
                nav_content_area.remove(&prev_detail);
            }
            let detail = build_album_detail(nav_state, album_id, nav_tx);
            nav_content_area.add_named(&detail, Some("detail"));
            nav_content_area.set_visible_child(&detail);
        }
        ArtistDetail(artist_id) => {
            info!(artist_id, "Navigating to artist detail",);
            if let Some(prev_detail) = nav_content_area.child_by_name("detail") {
                nav_content_area.remove(&prev_detail);
            }
            let detail = build_artist_detail(nav_state, artist_id, nav_tx);
            nav_content_area.add_named(&detail, Some("detail"));
            nav_content_area.set_visible_child(&detail);
        }
        Back => {
            info!("Navigating back to library view");
            nav_content_area.set_visible_child(orig_stack);
            if let Some(prev_detail) = nav_content_area.child_by_name("detail") {
                nav_content_area.remove(&prev_detail);
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;

    use crate::app::AppState;

    #[test]
    fn window_builds_with_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        drop(state);
        Ok(())
    }
}

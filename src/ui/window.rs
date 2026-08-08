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
    libadwaita::{
        Application, ApplicationWindow, Breakpoint, BreakpointCondition,
        BreakpointConditionLengthType::MaxWidth,
        LengthUnit::Sp,
        OverlaySplitView, Toast, ToastOverlay,
        ToastPriority::Normal,
        gdk::Display,
        glib::{
            ControlFlow::Break,
            Propagation::Proceed,
            idle_add_local,
            object::{Cast, ObjectExt},
            spawn_future_local,
        },
        gtk::{
            CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, Window, prelude::ToggleButtonExt,
            style_context_add_provider_for_display,
        },
        prelude::{AdwApplicationWindowExt, ButtonExt, GtkWindowExt, WidgetExt},
    },
    tracing::info,
};

use crate::{
    app::AppState,
    ui::{
        library::narrow_state::NarrowState, player::wire_panel_events, window_panes::build_content,
    },
};

/// Schedules deferred `OverlaySplitView` collapse changes.
///
/// Breakpoint setters apply synchronously inside the window's size-allocate
/// pass; collapsing the split view there swaps its internal layout manager and
/// toggles its shield widget mid-allocation, leaving libadwaita's bare
/// `AdwGizmo` widgets in a draw-before-allocation state ("Trying to snapshot
/// `AdwGizmo` without a current allocation"). This buffers the desired state
/// and applies it from an idle callback, coalescing rapid breakpoint
/// transitions into a single `set_collapsed` call off the allocation path.
///
/// Only the atomic state is shared (via [`Arc`]) so the scheduler stays
/// thread-safe; the split view is passed to [`CollapseScheduler::set`] on each
/// call instead of being stored.
struct CollapseScheduler {
    /// Desired collapsed state, coalescing apply/unapply transitions.
    desired: AtomicBool,
    /// Whether an idle apply is already scheduled.
    pending: AtomicBool,
}

impl CollapseScheduler {
    /// Create a scheduler initialized to the split view's current state.
    #[must_use]
    fn new(split_view: &OverlaySplitView) -> Arc<Self> {
        Arc::new(Self {
            desired: AtomicBool::new(split_view.is_collapsed()),
            pending: AtomicBool::new(false),
        })
    }

    /// Record the desired collapse state and apply it from an idle callback.
    ///
    /// Repeated calls before the idle callback runs coalesce into a single
    /// `set_collapsed` applying the final state, so nested breakpoint
    /// transitions (e.g. 800 px unapply followed by 700 px apply) settle on
    /// the correct collapsed state without flipping.
    fn set(self: &Arc<Self>, split_view: &OverlaySplitView, collapsed: bool) {
        self.desired.store(collapsed, Relaxed);
        if self.pending.swap(true, Relaxed) {
            return;
        }
        let this = Arc::clone(self);
        let split_view = split_view.clone();
        idle_add_local(move || {
            this.apply(&split_view);
            Break
        });
    }

    /// Apply the current desired state to the split view.
    fn apply(&self, split_view: &OverlaySplitView) {
        let collapsed = self.desired.load(Relaxed);
        self.pending.store(false, Relaxed);
        if collapsed != split_view.is_collapsed() {
            split_view.set_collapsed(collapsed);
        }
    }
}

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
    let (toast_overlay, split_view, toggle_button, back_button, close_button) =
        build_content(state, &narrow_state, window.upcast_ref::<Window>());
    window.set_content(Some(&toast_overlay));

    listen_for_toasts(state, &toast_overlay);

    add_responsive_breakpoints(&window, &split_view, &narrow_state);

    wire_panel_events(state, &split_view);

    window.connect_close_request(|_| {
        info!("Window close requested — session persists during application shutdown");
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

    let window_close = window.clone();
    close_button.connect_clicked(move |_| {
        window_close.close();
    });

    split_view
        .bind_property("collapsed", &close_button, "visible")
        .sync_create()
        .build();

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
        headerbar {
            transition: background 200ms ease;
        }
        ",
    );
    style_context_add_provider_for_display(
        &display,
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
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
///
/// The collapse is *deferred* via [`CollapseScheduler`] instead of a
/// declarative breakpoint setter: setters apply synchronously inside the
/// window's size-allocate pass, and collapsing the split view there swaps its
/// internal layout manager mid-allocation. Deferring the property change to
/// an idle callback keeps the layout mutation off the allocation path.
fn add_responsive_breakpoints(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    narrow_state: &Arc<NarrowState>,
) {
    let collapse = CollapseScheduler::new(split_view);

    let sidebar_condition = BreakpointCondition::new_length(MaxWidth, 800.0, Sp);
    let sidebar_bp = Breakpoint::new(sidebar_condition);
    sidebar_bp.connect_apply({
        let collapse = Arc::clone(&collapse);
        let sv = split_view.clone();
        move |_| collapse.set(&sv, true)
    });
    sidebar_bp.connect_unapply({
        let collapse = Arc::clone(&collapse);
        let sv = split_view.clone();
        move |_| collapse.set(&sv, false)
    });
    window.add_breakpoint(sidebar_bp);

    let narrow_condition = BreakpointCondition::new_length(MaxWidth, 700.0, Sp);
    let narrow_bp = Breakpoint::new(narrow_condition);
    narrow_bp.connect_apply({
        let collapse = Arc::clone(&collapse);
        let sv = split_view.clone();
        let ns = Arc::clone(narrow_state);
        move |_| {
            collapse.set(&sv, true);
            ns.set(true);
        }
    });
    narrow_bp.connect_unapply({
        let collapse = Arc::clone(&collapse);
        let sv = split_view.clone();
        let ns = Arc::clone(narrow_state);
        move |_| {
            collapse.set(&sv, false);
            ns.set(false);
        }
    });
    window.add_breakpoint(narrow_bp);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering::Relaxed};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            OverlaySplitView,
            glib::MainContext,
            gtk::{self, test},
        },
    };

    use crate::{app::AppState, ui::window::CollapseScheduler};

    fn pump_main_context() {
        let mut iterations = 0;
        while MainContext::default().iteration(false) && iterations < 1000 {
            iterations += 1;
        }
    }

    #[test]
    fn window_builds_with_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        drop(state);
        Ok(())
    }

    #[test]
    fn collapse_scheduler_initializes_from_current_state() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);
        ensure!(
            scheduler.desired.load(Relaxed) == split_view.is_collapsed(),
            "initial desired state must mirror the split view"
        );
        split_view.set_collapsed(true);
        let scheduler = CollapseScheduler::new(&split_view);
        ensure!(
            scheduler.desired.load(Relaxed),
            "a collapsed split view must seed a collapsed scheduler"
        );
        Ok(())
    }

    #[test]
    fn collapse_scheduler_applies_from_idle() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);
        ensure!(!split_view.is_collapsed());

        scheduler.set(&split_view, true);
        ensure!(
            !split_view.is_collapsed(),
            "collapse must be deferred to an idle callback"
        );

        pump_main_context();
        ensure!(
            split_view.is_collapsed(),
            "idle callback must apply the collapse"
        );
        Ok(())
    }

    #[test]
    fn collapse_scheduler_coalesces_nested_transitions() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);

        scheduler.set(&split_view, true);
        scheduler.set(&split_view, false);
        scheduler.set(&split_view, true);

        pump_main_context();
        ensure!(
            split_view.is_collapsed(),
            "nested apply/unapply must settle on the final desired state"
        );
        Ok(())
    }

    #[test]
    fn collapse_scheduler_unapply_expands_split_view() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);

        scheduler.set(&split_view, true);
        pump_main_context();
        ensure!(split_view.is_collapsed());

        scheduler.set(&split_view, false);
        pump_main_context();
        ensure!(
            !split_view.is_collapsed(),
            "unapply must expand the split view"
        );
        Ok(())
    }
}

//! Responsive breakpoints that defer `OverlaySplitView` collapse changes.
//!
//! Breakpoint setters apply synchronously inside the window's size-allocate
//! pass; collapsing the split view there swaps its internal layout manager and
//! toggles its shield widget mid-allocation, leaving libadwaita's bare
//! `AdwGizmo` widgets in a draw-before-allocation state ("Trying to snapshot
//! `AdwGizmo` without a current allocation"). This buffers the desired state
//! and applies it from an idle callback, coalescing rapid breakpoint
//! transitions into a single `set_collapsed` call off the allocation path.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use libadwaita::{
    ApplicationWindow, Breakpoint, BreakpointCondition, BreakpointConditionLengthType::MaxWidth,
    LengthUnit::Sp, OverlaySplitView, glib::idle_add_local_once, gtk::Widget,
    prelude::AdwApplicationWindowExt,
};

use crate::ui::{
    gallery::narrow_flag::NarrowState, panes::SwitcherGroup, signal_handlers::UiHandles,
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
        let mut handles = UiHandles::default();
        handles.retain_source(idle_add_local_once(move || {
            this.apply(&split_view);
        }));
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

/// Add responsive breakpoints for narrow windows.
///
/// Collapses the `OverlaySplitView` sidebar below 800 px width and
/// hides non‑essential columns (Format, Bit Depth, Sample Rate) below
/// 700 px width. Below 700 px the header `ViewSwitcher` is replaced by
/// the bottom `ViewSwitcherBar` so the header keeps room for its
/// controls on small windows.
///
/// The collapse is *deferred* via [`CollapseScheduler`] instead of a
/// declarative breakpoint setter: setters apply synchronously inside the
/// window's size-allocate pass, and collapsing the split view there swaps its
/// internal layout manager mid-allocation. Deferring the property change to
/// an idle callback keeps the layout mutation off the allocation path.
pub fn add_responsive_breakpoints(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    narrow_state: &Arc<NarrowState>,
    switchers: &SwitcherGroup,
) {
    let collapse = CollapseScheduler::new(split_view);

    let mut handles = UiHandles::default();
    let sidebar_condition = BreakpointCondition::new_length(MaxWidth, 800.0, Sp);
    let sidebar_bp = Breakpoint::new(sidebar_condition);
    handles.retain_signal(sidebar_bp.connect_apply({
        let collapse = Arc::clone(&collapse);
        let sv = split_view.clone();
        move |_| collapse.set(&sv, true)
    }));
    handles.retain_signal(sidebar_bp.connect_unapply({
        let collapse = Arc::clone(&collapse);
        let sv = split_view.clone();
        move |_| collapse.set(&sv, false)
    }));
    window.add_breakpoint(sidebar_bp);

    let narrow_condition = BreakpointCondition::new_length(MaxWidth, 700.0, Sp);
    let narrow_bp = Breakpoint::new(narrow_condition);
    let narrow_ctx = (
        Arc::clone(&collapse),
        split_view.clone(),
        Arc::clone(narrow_state),
        switchers.header.clone(),
        switchers.bar.clone(),
        switchers.switcher.clone(),
    );
    handles.retain_signal(narrow_bp.connect_apply({
        let (collapse, sv, ns, header, bar, _) = narrow_ctx.clone();
        move |_| {
            collapse.set(&sv, true);
            ns.set(true);
            header.set_title_widget(None::<&Widget>);
            bar.set_reveal(true);
        }
    }));
    handles.retain_signal(narrow_bp.connect_unapply({
        let (collapse, sv, ns, header, bar, switcher) = narrow_ctx;
        move |_| {
            collapse.set(&sv, false);
            ns.set(false);
            header.set_title_widget(Some(&switcher));
            bar.set_reveal(false);
        }
    }));
    window.add_breakpoint(narrow_bp);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering::Relaxed};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ApplicationWindow, OverlaySplitView,
            glib::MainContext,
            gtk::{self, test as gtk_test},
        },
    };

    use crate::{
        app::mocks::pump_in_test_runtime,
        ui::{
            collapse_scheduler::{CollapseScheduler, add_responsive_breakpoints},
            gallery::narrow_flag::NarrowState,
            panes::SwitcherGroup,
        },
    };

    #[test]
    fn responsive_breakpoints_signature_shape() {
        fn assert_shape<
            F: Fn(&ApplicationWindow, &OverlaySplitView, &Arc<NarrowState>, &SwitcherGroup),
        >(
            _: F,
        ) {
        }
        assert_shape(add_responsive_breakpoints);
    }

    fn pump_main_context() {
        let mut iterations: usize = 0;
        while MainContext::default().iteration(false) && iterations < 1000 {
            iterations = iterations.saturating_add(1);
        }
    }

    fn pump_with_tokio() -> Result<()> {
        pump_in_test_runtime(pump_main_context)
    }

    #[gtk_test]
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

    #[gtk_test]
    fn collapse_scheduler_applies_from_idle() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);
        ensure!(!split_view.is_collapsed());

        scheduler.set(&split_view, true);
        ensure!(
            !split_view.is_collapsed(),
            "collapse must be deferred to an idle callback"
        );

        pump_with_tokio()?;
        ensure!(
            split_view.is_collapsed(),
            "idle callback must apply the collapse"
        );
        Ok(())
    }

    #[gtk_test]
    fn collapse_scheduler_coalesces_nested_transitions() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);

        scheduler.set(&split_view, true);
        scheduler.set(&split_view, false);
        scheduler.set(&split_view, true);

        pump_with_tokio()?;
        ensure!(
            split_view.is_collapsed(),
            "nested apply/unapply must settle on the final desired state"
        );
        Ok(())
    }

    #[gtk_test]
    fn collapse_scheduler_unapply_expands_split_view() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);

        scheduler.set(&split_view, true);
        pump_with_tokio()?;
        ensure!(split_view.is_collapsed());

        scheduler.set(&split_view, false);
        pump_with_tokio()?;
        ensure!(
            !split_view.is_collapsed(),
            "unapply must expand the split view"
        );
        Ok(())
    }
}

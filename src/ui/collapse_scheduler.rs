//! Responsive breakpoints that defer `OverlaySplitView` collapse changes.
//!
//! Breakpoint setters apply synchronously inside the window's size-allocate
//! pass; collapsing the split view there swaps its internal layout manager and
//! toggles its shield widget mid-allocation, leaving libadwaita's bare
//! `AdwGizmo` widgets in a draw-before-allocation state ("Trying to snapshot
//! `AdwGizmo` without a current allocation"). The same holds for the narrow
//! breakpoint's companions (narrow flag, header title swap, `ViewSwitcherBar`
//! reveal), which would otherwise force `GtkRevealer`/`GtkScrolledWindow`
//! re-measurement mid-allocation ("Trying to measure ... for width of 30").
//! This buffers the desired state and applies it from an idle callback,
//! coalescing rapid breakpoint transitions into a single call off the
//! allocation path.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use libadwaita::{
    ApplicationWindow, Breakpoint, BreakpointCondition, BreakpointConditionLengthType::MaxWidth,
    LengthUnit::Sp, OverlaySplitView, glib::idle_add_local_once, prelude::AdwApplicationWindowExt,
};

use crate::ui::{
    gallery::narrow_flag::NarrowState, narrow_scheduler::NarrowScheduler, panes::SwitcherGroup,
    signal_handlers::UiHandles,
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
/// The collapse *and* the narrow-mode swaps are *deferred* via
/// [`CollapseScheduler`] and [`NarrowScheduler`] instead of declarative
/// breakpoint setters: setters apply synchronously inside the window's
/// size-allocate pass, and mutating layout there triggers measure warnings.
/// Deferring every property change to an idle callback keeps layout mutation
/// off the allocation path.
pub fn add_responsive_breakpoints(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    narrow_state: &Arc<NarrowState>,
    switchers: &SwitcherGroup,
) {
    let collapse = CollapseScheduler::new(split_view);
    let narrow_sched = NarrowScheduler::new(narrow_state);

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
        Arc::clone(&narrow_sched),
        Arc::clone(narrow_state),
        switchers.header.clone(),
        switchers.bar.clone(),
        switchers.switcher.clone(),
    );
    handles.retain_signal(narrow_bp.connect_apply({
        let (collapse, sv, sched, ns, header, bar, switcher) = narrow_ctx.clone();
        move |_| {
            collapse.set(&sv, true);
            sched.set(&ns, &header, &bar, &switcher, true);
        }
    }));
    handles.retain_signal(narrow_bp.connect_unapply({
        let (collapse, sv, sched, ns, header, bar, switcher) = narrow_ctx;
        move |_| {
            collapse.set(&sv, false);
            sched.set(&ns, &header, &bar, &switcher, false);
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
            ApplicationWindow, HeaderBar, OverlaySplitView, ViewStack, ViewSwitcher,
            ViewSwitcherBar,
            glib::MainContext,
            gtk::{self, test as gtk_test},
        },
    };

    use crate::{
        app::mocks::pump_in_test_runtime,
        ui::{
            collapse_scheduler::{CollapseScheduler, add_responsive_breakpoints},
            gallery::narrow_flag::NarrowState,
            narrow_scheduler::NarrowScheduler,
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

    fn drive_collapse(
        scheduler: &Arc<CollapseScheduler>,
        split_view: &OverlaySplitView,
        collapsed: bool,
    ) -> Result<()> {
        scheduler.set(split_view, collapsed);
        pump_with_tokio()?;
        ensure!(
            split_view.is_collapsed() == collapsed,
            "idle callback must settle the collapse on the final state"
        );
        Ok(())
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

        drive_collapse(&scheduler, &split_view, true)?;
        Ok(())
    }

    #[gtk_test]
    fn collapse_scheduler_coalesces_nested_transitions() -> Result<()> {
        let split_view = OverlaySplitView::new();
        let scheduler = CollapseScheduler::new(&split_view);

        for collapsed in [true, false, true] {
            scheduler.set(&split_view, collapsed);
        }

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

        drive_collapse(&scheduler, &split_view, true)?;
        drive_collapse(&scheduler, &split_view, false)?;
        Ok(())
    }

    fn narrow_fixtures() -> (Arc<NarrowState>, HeaderBar, ViewSwitcherBar, ViewSwitcher) {
        let narrow_state = NarrowState::new_shared();
        let stack = ViewStack::new();
        let header = HeaderBar::new();
        let switcher = ViewSwitcher::builder().stack(&stack).build();
        header.set_title_widget(Some(&switcher));
        let bar = ViewSwitcherBar::builder().stack(&stack).build();
        bar.set_reveal(false);
        (narrow_state, header, bar, switcher)
    }

    fn assert_narrow_settled(
        narrow_state: &Arc<NarrowState>,
        bar: &ViewSwitcherBar,
        narrow: bool,
    ) -> Result<()> {
        ensure!(
            narrow_state.get() == narrow,
            "idle callback must settle the narrow flag on the final state"
        );
        ensure!(
            bar.reveals() == narrow,
            "idle callback must settle the switcher bar on the final state"
        );
        Ok(())
    }

    fn drive_narrow(
        scheduler: &Arc<NarrowScheduler>,
        narrow_state: &Arc<NarrowState>,
        header: &HeaderBar,
        bar: &ViewSwitcherBar,
        switcher: &ViewSwitcher,
        narrow: bool,
    ) -> Result<()> {
        scheduler.set(narrow_state, header, bar, switcher, narrow);
        pump_with_tokio()?;
        assert_narrow_settled(narrow_state, bar, narrow)
    }

    #[gtk_test]
    fn narrow_scheduler_defers_swaps_to_idle() -> Result<()> {
        let (narrow_state, header, bar, switcher) = narrow_fixtures();
        let scheduler = NarrowScheduler::new(&narrow_state);

        scheduler.set(&narrow_state, &header, &bar, &switcher, true);
        ensure!(
            !narrow_state.get(),
            "narrow flag must not flip synchronously inside the breakpoint setter"
        );
        ensure!(
            !bar.reveals(),
            "switcher bar must not reveal synchronously inside the breakpoint setter"
        );

        pump_with_tokio()?;
        assert_narrow_settled(&narrow_state, &bar, true)?;
        ensure!(
            header.title_widget().is_none(),
            "idle callback must clear the header title widget"
        );
        Ok(())
    }

    #[gtk_test]
    fn narrow_scheduler_coalesces_nested_transitions() -> Result<()> {
        let (narrow_state, header, bar, switcher) = narrow_fixtures();
        let scheduler = NarrowScheduler::new(&narrow_state);

        for narrow in [true, false, true] {
            scheduler.set(&narrow_state, &header, &bar, &switcher, narrow);
        }

        pump_with_tokio()?;
        assert_narrow_settled(&narrow_state, &bar, true)?;
        Ok(())
    }

    #[gtk_test]
    fn narrow_scheduler_unapply_restores_header() -> Result<()> {
        let (narrow_state, header, bar, switcher) = narrow_fixtures();
        let scheduler = NarrowScheduler::new(&narrow_state);

        drive_narrow(&scheduler, &narrow_state, &header, &bar, &switcher, true)?;

        drive_narrow(&scheduler, &narrow_state, &header, &bar, &switcher, false)?;
        ensure!(
            header.title_widget().is_some(),
            "unapply must restore the header title widget"
        );
        Ok(())
    }
}

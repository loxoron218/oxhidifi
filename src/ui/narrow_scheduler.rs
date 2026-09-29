//! Deferred narrow-mode UI changes for the 700 px breakpoint.
//!
//! Breakpoint setters apply synchronously inside the window's size-allocate
//! pass; swapping the header title widget, revealing the bottom
//! `ViewSwitcherBar`, and flipping the [`NarrowState`] flag there forces
//! `GtkRevealer`/`GtkScrolledWindow` re-measurement mid-allocation
//! ("Trying to measure ... for width of 30"). This buffers the desired flag
//! and applies all three mutations from a single idle callback, coalescing
//! rapid breakpoint transitions off the allocation path.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use libadwaita::{
    HeaderBar, ViewSwitcher, ViewSwitcherBar, glib::idle_add_local_once, gtk::Widget,
};

use crate::ui::{gallery::narrow_flag::NarrowState, signal_handlers::UiHandles};

/// Schedules deferred narrow-mode UI changes.
///
/// Like [`CollapseScheduler`](crate::ui::collapse_scheduler::CollapseScheduler),
/// doing the 700 px breakpoint swaps synchronously inside the breakpoint
/// setter (i.e. inside size-allocate) forces `GtkRevealer`/`GtkScrolledWindow`
/// re-measurement mid-allocation. This buffers the desired flag and applies
/// all three mutations from a single idle callback.
///
/// Only the atomic state is shared (via [`Arc`]) so the scheduler stays
/// thread-safe; the widgets are passed to [`NarrowScheduler::set`] on each
/// call instead of being stored.
#[derive(Debug)]
pub struct NarrowScheduler {
    /// Desired narrow state, coalescing apply/unapply transitions.
    desired: AtomicBool,
    /// Whether an idle apply is already scheduled.
    pending: AtomicBool,
}

impl NarrowScheduler {
    /// Create a scheduler initialized to the current narrow state.
    pub fn new(narrow_state: &Arc<NarrowState>) -> Arc<Self> {
        Arc::new(Self {
            desired: AtomicBool::new(narrow_state.get()),
            pending: AtomicBool::new(false),
        })
    }

    /// Record the desired narrow state and apply it from an idle callback.
    ///
    /// Repeated calls before the idle callback runs coalesce into a single
    /// apply of the final state. Widgets are cloned into the idle closure so
    /// the setter itself never touches layout.
    pub fn set(
        self: &Arc<Self>,
        narrow_state: &Arc<NarrowState>,
        header: &HeaderBar,
        bar: &ViewSwitcherBar,
        switcher: &ViewSwitcher,
        narrow: bool,
    ) {
        self.desired.store(narrow, Relaxed);
        if self.pending.swap(true, Relaxed) {
            return;
        }
        let this = Arc::clone(self);
        let state = Arc::clone(narrow_state);
        let header = header.clone();
        let bar = bar.clone();
        let switcher = switcher.clone();
        let mut handles = UiHandles::default();
        handles.retain_source(idle_add_local_once(move || {
            this.apply(&state, &header, &bar, &switcher);
        }));
    }

    /// Apply the current desired narrow state off the allocation path.
    fn apply(
        &self,
        narrow_state: &NarrowState,
        header: &HeaderBar,
        bar: &ViewSwitcherBar,
        switcher: &ViewSwitcher,
    ) {
        let narrow = self.desired.load(Relaxed);
        self.pending.store(false, Relaxed);
        narrow_state.set(narrow);
        if narrow {
            header.set_title_widget(None::<&Widget>);
            bar.set_reveal(true);
        } else {
            header.set_title_widget(Some(switcher));
            bar.set_reveal(false);
        }
    }
}

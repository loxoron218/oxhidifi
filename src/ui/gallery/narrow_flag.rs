//! Narrow-width mode tracking for adaptive column hiding.

use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use async_channel::Receiver;

use crate::ui::signal_handlers::ValueSignal;

/// Tracks whether the window is in narrow‑width mode.
///
/// Created via [`NarrowState::new_shared`] and shared via [`Arc`].
/// Subscribe to changes with [`NarrowState::subscribe`]. Subscribers receive
/// updates through an `async_channel`, which wakes the `GLib` main context
/// reliably when awaited in a `spawn_future_local`.
pub struct NarrowState {
    /// Whether the window is in narrow mode.
    narrow: AtomicBool,
    /// Signals subscribers of narrow-mode changes.
    narrow_signal: ValueSignal<bool>,
}

impl NarrowState {
    /// Create a new `NarrowState` wrapped in an [`Arc`].
    #[must_use]
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self {
            narrow: AtomicBool::new(false),
            narrow_signal: ValueSignal::new(false),
        })
    }

    /// Set the narrow state and notify all subscribers.
    pub fn set(&self, val: bool) {
        self.narrow.store(val, Relaxed);
        self.narrow_signal.send(val);
    }

    /// Return the current narrow state.
    pub fn get(&self) -> bool {
        self.narrow.load(Relaxed)
    }

    /// Subscribe to narrow state changes.
    ///
    /// Returns an `async_channel` receiver that yields the new value on every
    /// change; the current value is available synchronously via
    /// [`NarrowState::get`].
    pub fn subscribe(&self) -> Receiver<bool> {
        self.narrow_signal.subscribe()
    }
}

impl Debug for NarrowState {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("NarrowState")
            .field("narrow", &self.narrow.load(Relaxed))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::gallery::narrow_flag::NarrowState;

    #[test]
    fn narrow_state_defaults_false() {
        let state = NarrowState::new_shared();
        assert!(!state.get(), "narrow state should default to false");
    }

    #[test]
    fn narrow_state_set_and_get() {
        let state = NarrowState::new_shared();
        state.set(true);
        assert!(state.get(), "narrow state should reflect set(true)");
        state.set(false);
        assert!(!state.get(), "narrow state should reflect set(false)");
    }

    #[test]
    fn narrow_state_subscriber_receives_changes() -> Result<()> {
        let state = NarrowState::new_shared();
        let rx = state.subscribe();
        ensure!(!state.get(), "subscriber should see the initial value");
        state.set(true);
        ensure!(matches!(rx.try_recv(), Ok(true)));
        state.set(false);
        ensure!(matches!(rx.try_recv(), Ok(false)));
        Ok(())
    }
}

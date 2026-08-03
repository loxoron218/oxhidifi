//! Narrow-width mode tracking for adaptive column hiding.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use {
    tokio::sync::watch::{Receiver, Sender as TokioSender, channel as TokioChannel},
    tracing::warn,
};

/// Tracks whether the window is in narrow‑width mode.
///
/// Created via [`NarrowState::new_shared`] and shared via [`Arc`].
/// Subscribe to changes with [`NarrowState::subscribe`].
pub struct NarrowState {
    /// Whether the window is in narrow mode.
    narrow: AtomicBool,
    /// Channel to notify subscribers of narrow-mode changes.
    tx: TokioSender<bool>,
    /// Kept alive so [`send`] never fails when no external subscribers exist.
    rx: Receiver<bool>,
}

impl NarrowState {
    /// Create a new `NarrowState` wrapped in an [`Arc`].
    #[must_use]
    pub fn new_shared() -> Arc<Self> {
        let (tx, rx) = TokioChannel(false);
        Arc::new(Self {
            narrow: AtomicBool::new(false),
            tx,
            rx,
        })
    }

    /// Set the narrow state and notify all subscribers.
    pub fn set(&self, val: bool) {
        self.narrow.store(val, Relaxed);
        if let Err(e) = self.tx.send(val) {
            warn!(error = %e, "No narrow state subscribers");
        }
    }

    /// Return the current narrow state.
    pub fn get(&self) -> bool {
        self.narrow.load(Relaxed)
    }

    /// Subscribe to narrow state changes.
    ///
    /// The receiver will immediately yield the current value on first
    /// [`changed`](watch::Receiver::changed) call.
    pub fn subscribe(&self) -> Receiver<bool> {
        self.rx.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::library::narrow_state::NarrowState;

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
    fn narrow_state_subscriber_receives_changes() {
        let state = NarrowState::new_shared();
        let rx = state.subscribe();
        assert!(!*rx.borrow(), "subscriber should see the initial value");
        state.set(true);
        assert!(*rx.borrow(), "subscriber should see the new value");
        state.set(false);
        assert!(!*rx.borrow(), "subscriber should see the toggled value");
    }
}

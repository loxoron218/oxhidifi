//! Latest-value store with per-subscriber unbounded signal fan-out.

use std::sync::Arc;

use {
    async_channel::{Receiver, Sender, unbounded},
    parking_lot::Mutex,
};

/// Latest-value store with per-subscriber unbounded `async_channel` fan-out.
///
/// Subscribers registered via [`ValueSignal::subscribe`] receive the current
/// value on every [`ValueSignal::send`] or [`ValueSignal::publish`] through
/// an `async_channel`, which wakes the `GLib` main context reliably when the
/// receiver is awaited in a `spawn_future_local` — unlike `tokio::sync::watch`,
/// whose wakers do not reliably wake the `GLib` main loop. Slow or dropped
/// subscribers never affect others: each has its own unbounded channel.
pub struct ValueSignal<T> {
    /// Inner shared state.
    inner: Arc<ValueSignalInner<T>>,
}

impl<T: Copy> ValueSignal<T> {
    /// Create a channel with the given initial value.
    pub fn new(initial: T) -> Self {
        Self {
            inner: Arc::new(ValueSignalInner {
                value: Mutex::new(initial),
                subs: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Read the current value synchronously.
    #[must_use]
    pub fn borrow(&self) -> T {
        *self.inner.value.lock()
    }

    /// Update the value and notify every subscriber.
    pub fn send(&self, value: T) {
        *self.inner.value.lock() = value;
        self.publish();
    }

    /// Notify every subscriber with the current value without changing it.
    pub fn publish(&self) {
        let value = *self.inner.value.lock();
        let mut subs = self.inner.subs.lock();
        subs.retain(|tx| tx.try_send(value).is_ok());
    }

    /// Subscribe to value changes.
    ///
    /// Returns an `async_channel` receiver that yields the value on every
    /// subsequent [`send`](Self::send)/[`publish`](Self::publish). The sender
    /// is pruned from the fan-out when the receiver is dropped.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<T> {
        let (tx, rx) = unbounded();
        self.inner.subs.lock().push(tx);
        rx
    }
}

impl<T> Clone for ValueSignal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Shared state behind a [`ValueSignal`].
struct ValueSignalInner<T> {
    /// Authoritative latest value, readable synchronously via [`ValueSignal::borrow`].
    value: Mutex<T>,
    /// One unbounded sender per subscriber; pruned when a receiver is dropped.
    subs: Mutex<Vec<Sender<T>>>,
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::signal::ValueSignal;

    #[test]
    fn borrow_returns_initial_value() {
        let channel = ValueSignal::new(7u8);
        assert_eq!(channel.borrow(), 7);
    }

    #[test]
    fn send_updates_value_and_notifies() -> Result<()> {
        let channel = ValueSignal::new(1u8);
        let rx = channel.subscribe();
        channel.send(2);
        ensure!(channel.borrow() == 2);
        ensure!(matches!(rx.try_recv(), Ok(2)));
        Ok(())
    }

    #[test]
    fn publish_notifies_without_changing_value() -> Result<()> {
        let channel = ValueSignal::new(3u8);
        let rx = channel.subscribe();
        channel.publish();
        ensure!(channel.borrow() == 3);
        ensure!(matches!(rx.try_recv(), Ok(3)));
        Ok(())
    }

    #[test]
    fn dropped_receiver_is_pruned() -> Result<()> {
        let channel = ValueSignal::new(0u8);
        let rx = channel.subscribe();
        ensure!(channel.inner.subs.lock().len() == 1);
        drop(rx);
        channel.publish();
        ensure!(
            channel.inner.subs.lock().is_empty(),
            "dropped receivers must be pruned on publish"
        );
        Ok(())
    }

    #[test]
    fn multiple_subscribers_each_receive_value() -> Result<()> {
        let channel = ValueSignal::new(0u8);
        let rx1 = channel.subscribe();
        let rx2 = channel.subscribe();
        channel.send(9);
        ensure!(matches!(rx1.try_recv(), Ok(9)));
        ensure!(matches!(rx2.try_recv(), Ok(9)));
        Ok(())
    }
}

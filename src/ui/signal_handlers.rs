//! Latest-value store with per-subscriber unbounded signal fan-out.
//!
//! Also owns fire-and-forget GTK handles: signal connections, main-context
//! tasks, event sources, and property bindings return small handle values
//! that must be owned for the application lifetime (see [`UiHandles`]).

use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    sync::Arc,
};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::glib::{Binding, JoinHandle, SignalHandlerId, SourceId},
    parking_lot::Mutex,
    tokio::task::JoinHandle as TokioJoinHandle,
};

/// Owns fire-and-forget UI handles for the application lifetime.
///
/// Centralizes ownership of every handle that outlives its creation scope:
/// signal connections live with their widgets, tasks and sources with the
/// main context or Tokio runtime, and property bindings live exactly as long
/// as the [`Binding`] value is retained (dropping one unbinds it). All
/// `retain_*` methods return `()`, so call sites stay lint-clean without
/// placeholder bindings. Thread safety comes from the `parking_lot::Mutex`
/// at the storage site; this type itself is only touched on the GTK main
/// thread.
#[derive(Default)]
pub struct UiHandles {
    /// Retained property bindings; each stays bound while stored here.
    bindings: Vec<Binding>,
    /// Retained Tokio task handles, detached but owned for symmetry.
    blocking_tasks: Vec<TokioJoinHandle<()>>,
    /// Retained signal handler IDs, kept as explicit ownership records.
    signals: Vec<SignalHandlerId>,
    /// Retained event source IDs.
    sources: Vec<SourceId>,
    /// Retained main-context task handles.
    tasks: Vec<JoinHandle<()>>,
}

impl UiHandles {
    /// Retain a property binding so it stays bound.
    ///
    /// # Arguments
    ///
    /// * `binding` - Binding returned by `bind_property().build()`; dropping it would unbind the
    ///   properties immediately.
    pub fn retain_binding(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    /// Retain a spawned Tokio task handle.
    ///
    /// # Arguments
    ///
    /// * `handle` - Task handle returned by `tokio::spawn`; the task is detached and keeps running
    ///   independently.
    pub fn retain_blocking_task(&mut self, handle: TokioJoinHandle<()>) {
        self.blocking_tasks.push(handle);
    }

    /// Retain a connected signal handler ID for the application lifetime.
    ///
    /// # Arguments
    ///
    /// * `id` - Handler ID returned by `connect_*`; the connection itself is kept alive by its
    ///   widget.
    pub fn retain_signal(&mut self, id: SignalHandlerId) {
        self.signals.push(id);
    }

    /// Retain an event source ID for the application lifetime.
    ///
    /// # Arguments
    ///
    /// * `id` - Source ID returned by `idle_add_local` or `timeout_add_local`; the source lives
    ///   with the main context.
    pub fn retain_source(&mut self, id: SourceId) {
        self.sources.push(id);
    }

    /// Retain a spawned main-context task handle.
    ///
    /// # Arguments
    ///
    /// * `handle` - Task handle returned by `spawn_future_local` or `spawn_local`; the task keeps
    ///   running independently.
    pub fn retain_task(&mut self, handle: JoinHandle<()>) {
        self.tasks.push(handle);
    }
}

impl Debug for UiHandles {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("UiHandles")
            .field("signals", &self.signals.len())
            .field("tasks", &self.tasks.len())
            .field("blocking_tasks", &self.blocking_tasks.len())
            .field("sources", &self.sources.len())
            .field("bindings", &self.bindings.len())
            .finish()
    }
}

/// Latest-value store with per-subscriber unbounded `async_channel` fan-out.
///
/// Subscribers registered via [`ValueSignal::subscribe`] receive the current
/// value on every [`ValueSignal::send`] or [`ValueSignal::publish`] through
/// an `async_channel`, which wakes the `GLib` main context reliably when the
/// receiver is awaited in a `spawn_future_local` — unlike `tokio::sync::watch`,
/// whose wakers do not reliably wake the `GLib` main loop. Slow or dropped
/// subscribers never affect others: each has its own unbounded channel.
#[derive(Debug)]
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

    fn clone_from(&mut self, source: &Self) {
        Arc::clone_from(&mut self.inner, &source.inner);
    }
}

/// Shared state behind a [`ValueSignal`].
#[derive(Debug)]
struct ValueSignalInner<T> {
    /// Authoritative latest value, readable synchronously via [`ValueSignal::borrow`].
    value: Mutex<T>,
    /// One unbounded sender per subscriber; pruned when a receiver is dropped.
    subs: Mutex<Vec<Sender<T>>>,
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::signal_handlers::ValueSignal;

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

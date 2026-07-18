//! Thread lifecycle management and model documentation.
//!
//! # Thread Model
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │                        THREAD MODEL                              │
//! ├──────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  [1] MAIN THREAD (GLib Main Context)                             │
//! │      • GTK/Libadwaita rendering and input handling               │
//! │      • glib::spawn_future_local — async tasks                    │
//! │      • MainContext::default().spawn_local — async tasks          │
//! │      • glib::idle_add_local — deferred UI updates                │
//! │                                                                  │
//! │  [2] TOKIO RUNTIME (multi-threaded, n_cores workers)             │
//! │      • tokio::spawn — async tasks: metadata, scanning,           │
//! │        events, watcher consumer                                  │
//! │      • tokio::task::spawn_blocking — blocking I/O,               │
//! │        device enum, memory sampling, file writes                 │
//! │      • SQLx queries (all .await on storage)                      │
//! │                                                                  │
//! │  [3] DEDICATED OS THREADS (std::thread, Builder::new().name())   │
//! │      • Decode thread: decode → resample → rtrb push              │
//! │        Named "decode-{track_id}" in engine.rs                    │
//! │        JoinHandle stored in EngineShared::decode_thread          │
//! │        NOT joined during operation (blocking AudioOutput::drop)  │
//! │      • Cover decoder: single worker thread                       │
//! │        Named "cover-decoder" via ThreadManager                   │
//! │        Processes ArtworkDecodeRequest sequentially               │
//! │        Grid results via async_channel to spawn_future_local      │
//! │        (rx.recv().await — receiver alive until channel closes)   │
//! │        Column/detail results via idle_add_local polling          │
//! │      • Each thread handles AudioOutput lifecycle (blocking)      │
//! │      • Exactly one decode thread active at a time                │
//! │                                                                  │
//! │  [4] CPAL AUDIO CALLBACK (OS audio thread)                       │
//! │      • rtrb::Consumer (lock-free pop)                            │
//! │      • AtomicBool for flush/drain signal                         │
//! │      • NEVER holds a Mutex — real-time safety invariant          │
//! │                                                                  │
//! │  [5] RAYON THREAD POOL (n_cores workers)                         │
//! │      • Parallel directory walk and metadata extraction           │
//! │      • Runs inside spawn_blocking — intentional isolation        │
//! │                                                                  │
//! │  [6] NOTIFY WATCHER (OS thread, from notify crate)               │
//! │      • Callback → tokio::sync::mpsc::unbounded                   │
//! │      • Consumed by a tokio::spawn watcher task                   │
//! │                                                                  │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Channel Topology
//!
//! ```text
//! Engine ──────tokio::sync::mpsc (cap 4)──────> Decode thread (commands)
//! Decode ───────────rtrb::RingBuffer (96k)───────> CPAL callback (samples)
//! Engine ───────async_channel::unbounded ───────> PlaybackEvent subscribers
//! Scanner ──────async_channel::unbounded ───────> UI status bar (ScanEvent)
//! App ──────────async_channel::unbounded ───────> UI (toasts, navigation)
//! CoverArtCache ──async_channel::unbounded ─────> Cover decoder thread
//! Tokio task ────async_channel::unbounded ──────> GLib MainContext (results)
//! Notify ────────tokio::sync::mpsc::unbounded ──> Tokio watcher task
//! App ───────────tokio::sync::watch (1) ───────> UI (view_mode, active_tab,
//! │                                               refresh signal)
//! NarrowState ───tokio::sync::watch (1) ───────> ColumnView (narrow flag)
//! Scanner ───────tokio::sync::watch (1) ───────> Cancel signal
//! ```
//!
//! # Shutdown Sequence
//!
//! 1. `GLib` main loop exits (`app.run()` returns)
//! 2. `ThreadManager::shutdown()` joins the cover decoder thread
//! 3. `decode_tx` (command sender) dropped — decode thread sees channel disconnect and exits the
//!    decode loop
//! 4. `AudioOutput::drop()` runs inside the decode thread (blocking ALSA close). The decode thread
//!    is detached — its `JoinHandle` is stored in `EngineShared::decode_thread` but never
//!    explicitly joined to avoid blocking the `GLib` main loop.
//! 5. Tokio runtime drops → all tokio tasks cancelled
//! 6. Rayon pool drains
//! 7. Process exits

//! # Exceptions
//!
//! - The **decode thread** (`engine.rs`) is intentionally NOT managed via `ThreadManager`. Its
//!   `AudioOutput::drop()` is blocking and must not be joined from the `GLib` main thread. Its
//!   `JoinHandle` is stored directly in `EngineShared::decode_thread`.

use std::{
    mem::take,
    thread::{Builder, JoinHandle},
};

use {parking_lot::Mutex, tracing::error};

/// Manages named OS thread lifecycle.
///
/// Provides named thread spawning and `JoinHandle` tracking for
/// graceful shutdown. Used for the cover decoder thread in
/// `CoverArtCache::new_shared`.
///
/// The decode thread (`engine.rs`) is intentionally NOT managed here
/// because its `AudioOutput::drop()` is blocking and must not be
/// joined from the `GLib` main thread. Its `JoinHandle` lives in
/// `EngineShared::decode_thread` instead. See the module-level docs
/// for the full threading model.
pub struct ThreadManager {
    /// Collected join handles for all spawned threads.
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl ThreadManager {
    /// Create a new empty `ThreadManager`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
        }
    }

    /// Spawn a named OS thread.
    ///
    /// The thread is registered in the manager and will be joined
    /// during [`shutdown`](Self::shutdown).
    pub fn spawn_named<F>(&self, name: &str, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Ok(handle) = Builder::new().name(name.into()).spawn(f) {
            self.handles.lock().push(handle);
        }
    }

    /// Join all tracked threads.
    ///
    /// # Blocking
    ///
    /// This call blocks until every tracked thread has exited.
    /// Must not be called from the `GLib` main thread — use
    /// `spawn_blocking` or call during application teardown
    /// after the main loop returns.
    pub fn shutdown(&self) {
        let handles = take(&mut *self.handles.lock());
        for handle in handles {
            Self::join_quietly(handle);
        }
    }

    /// Join a single thread handle, logging on panic.
    fn join_quietly(handle: JoinHandle<()>) {
        if let Err(e) = handle.join() {
            error!(error = ?e, "Thread panicked");
        }
    }
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
        },
        thread::current,
    };

    use crate::threading::ThreadManager;

    fn set_flag(f: &Arc<AtomicBool>) {
        f.store(true, SeqCst);
    }

    fn inc_atomic(c: &Arc<AtomicUsize>) {
        c.fetch_add(1, SeqCst);
    }

    #[test]
    fn thread_manager_spawns_and_joins() {
        let mgr = ThreadManager::new();
        let flag = Arc::new(AtomicBool::new(false));
        let f = Arc::clone(&flag);
        mgr.spawn_named("test-thread", move || set_flag(&f));
        mgr.shutdown();
        assert!(flag.load(SeqCst));
    }

    #[test]
    fn thread_manager_names_thread() {
        let mgr = ThreadManager::new();
        mgr.spawn_named("my-named-thread", || {
            assert_eq!(current().name(), Some("my-named-thread"));
        });
        mgr.shutdown();
    }

    #[test]
    fn shutdown_empty_does_not_panic() {
        let mgr = ThreadManager::new();
        mgr.shutdown();
    }

    #[test]
    fn shutdown_multiple_threads() {
        let mgr = ThreadManager::new();
        let counter = Arc::new(AtomicUsize::new(0));
        for i in 0..5 {
            let c = Arc::clone(&counter);
            let name = format!("worker-{i}");
            mgr.spawn_named(&name, move || inc_atomic(&c));
        }
        mgr.shutdown();
        assert_eq!(counter.load(SeqCst), 5);
    }

    #[test]
    fn thread_manager_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ThreadManager>();
        assert_sync::<ThreadManager>();
    }

    #[test]
    fn spawn_named_accepts_multiple_threads() {
        let mgr = ThreadManager::new();
        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let c = Arc::clone(&counter);
            mgr.spawn_named("worker", move || inc_atomic(&c));
        }
        mgr.shutdown();
        assert_eq!(counter.load(SeqCst), 3);
    }
}

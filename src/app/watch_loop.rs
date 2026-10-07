//! Filesystem watcher event loop with burst coalescing.
//!
//! Drains buffered watcher events to distinct scan targets before
//! processing, so copying an album (one event per file) triggers one parent
//! scan instead of one scan per file.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use tokio::{spawn, sync::mpsc::UnboundedReceiver, task::JoinHandle};

use crate::{
    library::{
        watcher::LibraryWatcher,
        watcher_event::{
            WatcherEvent::{self, DirectoryModified, Error},
            scan_target_for,
        },
    },
    storage::database::SqliteStorage,
};

/// Run the filesystem watcher loop in the background.
///
/// Takes `Arc<LibraryWatcher>` with interior mut for `watch`/`unwatch`.
/// Returns a `JoinHandle` so the caller can await graceful shutdown after
/// calling [`LibraryWatcher::shutdown`]. The loop exits when the channel
/// closes (shared `event_tx` taken) and discards queued events after a
/// cancellation signal.
///
/// Bursts (e.g. copying an album emits one event per file) are coalesced by
/// draining the channel to distinct scan targets before processing, so N
/// files in one parent trigger one parent scan instead of N scans.
async fn watcher_loop(
    watcher: Arc<LibraryWatcher<SqliteStorage>>,
    mut watcher_rx: UnboundedReceiver<WatcherEvent>,
) {
    while !watcher.is_cancelled() {
        let Some(first) = watcher_rx.recv().await else {
            break;
        };
        let events = coalesce_watcher_events(first, &mut watcher_rx);
        process_watcher_batch(&watcher, events).await;
    }
}

/// Process one coalesced watcher batch sequentially.
///
/// # Arguments
///
/// * `watcher` - Filesystem watcher handling each event.
/// * `events` - Distinct events from [`coalesce_watcher_events`].
async fn process_watcher_batch(
    watcher: &Arc<LibraryWatcher<SqliteStorage>>,
    events: Vec<WatcherEvent>,
) {
    for event in events {
        if watcher.is_cancelled() {
            return;
        }
        watcher.process_event(event).await;
    }
}

/// Drain buffered watcher events and deduplicate directory scans.
///
/// # Arguments
///
/// * `first` - Event already received from the channel.
/// * `rx` - Channel to drain for further buffered events.
///
/// # Returns
///
/// Distinct events to process: one per scan target plus any errors. File
/// events resolve to their parent directory so N files in one album
/// collapse to a single parent scan.
fn coalesce_watcher_events(
    first: WatcherEvent,
    rx: &mut UnboundedReceiver<WatcherEvent>,
) -> Vec<WatcherEvent> {
    let mut buffered = vec![first];
    while let Ok(event) = rx.try_recv() {
        buffered.push(event);
    }
    let mut errors = Vec::new();
    let mut seen_targets: HashSet<PathBuf> = HashSet::new();
    let mut targets: HashMap<PathBuf, PathBuf> = HashMap::new();
    for event in buffered {
        collect_watcher_event(event, &mut errors, &mut seen_targets, &mut targets);
    }
    let mut coalesced: Vec<WatcherEvent> = targets
        .into_values()
        .map(|path| DirectoryModified { path })
        .collect();
    coalesced.extend(errors);
    coalesced
}

/// Collect one buffered event into the coalesced error and target maps.
///
/// # Arguments
///
/// * `event` - Buffered watcher event.
/// * `errors` - Sink for watcher errors, always processed.
/// * `seen_targets` - Scan targets already collected, for deduplication.
/// * `targets` - Scan target to original event path map.
fn collect_watcher_event(
    event: WatcherEvent,
    errors: &mut Vec<WatcherEvent>,
    seen_targets: &mut HashSet<PathBuf>,
    targets: &mut HashMap<PathBuf, PathBuf>,
) {
    match event {
        Error { .. } => errors.push(event),
        DirectoryModified { path } => {
            let target = scan_target_for(&path);
            if seen_targets.insert(target.clone()) {
                drop(targets.insert(target, path));
            }
        }
    }
}

/// Run the filesystem watcher loop in the background.
///
/// Takes `Arc<LibraryWatcher>` with interior mut for `watch`/`unwatch`.
/// Returns a `JoinHandle` so the caller can await graceful shutdown after
/// calling [`LibraryWatcher::shutdown`]. The loop exits when the channel
/// closes (shared `event_tx` taken) and discards queued events after a
/// cancellation signal.
pub fn spawn_watcher_loop(
    watcher: Arc<LibraryWatcher<SqliteStorage>>,
    watcher_rx: UnboundedReceiver<WatcherEvent>,
) -> JoinHandle<()> {
    spawn(watcher_loop(watcher, watcher_rx))
}

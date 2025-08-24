use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc,
        mpsc::{Receiver, channel},
    },
    thread::{sleep, spawn},
    time::{Duration, Instant},
};

use notify::{
    Event,
    EventKind::{Create, Modify, Remove},
    RecommendedWatcher,
    RecursiveMode::Recursive,
    Watcher,
    event::ModifyKind::{Data, Metadata, Name},
    recommended_watcher,
};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::data::{db::query::fetch_all_folders, scanner::library_ops::run_full_scan};

/// The duration to wait after a file system event before triggering a full library scan.
/// This helps to debounce rapid file changes and prevents excessive scanning.
const DEBOUNCE_DURATION: Duration = Duration::from_secs(5);

/// Spawns new threads for watching library folders and processing file events.
///
/// This function sets up a multi-threaded system for monitoring file system changes.
/// One thread listens for `notify` events and sends them to a channel. A second thread
/// receives these events, debounces them, and triggers a full library scan when the
/// file system has been quiet for a specified duration.
///
/// # Arguments
///
/// * `pool` - An `Arc` to the SQLite database pool.
/// * `sender` - An `UnboundedSender` to signal the UI for a refresh.
pub fn start_watching_library(pool: Arc<SqlitePool>, sender: UnboundedSender<()>) {
    // Create a channel for communication between the watcher and the event processor.
    let (tx, rx) = channel();

    // Spawn the event processing thread.
    let pool_clone_processor = pool.clone();
    let sender_clone_processor = sender.clone();
    spawn(move || {
        process_events(rx, pool_clone_processor, sender_clone_processor);
    });

    // Spawn the file system watcher thread.
    spawn(move || {
        let rt = Runtime::new().expect("Failed to create Tokio runtime for watcher thread");

        // Fetch all library folders to watch.
        let folders_to_watch = rt.block_on(async {
            fetch_all_folders(&pool).await.unwrap_or_else(|e| {
                eprintln!("Error fetching folders for watcher: {}", e);
                Vec::new()
            })
        });

        // If no folders are configured, there's nothing to watch, so the thread can exit.
        if folders_to_watch.is_empty() {
            eprintln!("No folders configured to watch. Watcher thread exiting.");
            return;
        }

        // Create the file system watcher with the channel sender.
        let mut watcher: RecommendedWatcher = match recommended_watcher(move |res| {
            if let Ok(event) = res {
                tx.send(event).expect("Failed to send event");
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create watcher: {}", e);
                return;
            }
        };

        // Add each folder to the watcher.
        for folder in folders_to_watch {
            let path = Path::new(&folder.path);
            if let Err(e) = watcher.watch(path, Recursive) {
                eprintln!("Failed to watch path {:?}: {}", path, e);
            }
        }

        // Keep the watcher thread alive.
        loop {
            sleep(Duration::from_secs(60));
        }
    });
}

/// Processes file system events with debouncing logic.
///
/// This function runs in a dedicated thread, receiving events from the watcher.
/// It collects events and waits for a quiet period (`DEBOUNCE_DURATION`) before
/// triggering a full library scan. This prevents excessive scans during periods
/// of high file activity.
fn process_events(rx: Receiver<Event>, pool: Arc<SqlitePool>, sender: UnboundedSender<()>) {
    let mut last_event_time = Instant::now();
    let mut event_queue: VecDeque<Event> = VecDeque::new();
    let rt = Arc::new(Runtime::new().expect("Failed to create Tokio runtime for event processor"));
    loop {
        // Try to receive an event from the channel.
        if let Ok(event) = rx.try_recv() {
            if matches!(
                event.kind,
                Create(_) | Remove(_) | Modify(Name(_)) | Modify(Data(_)) | Modify(Metadata(_))
            ) {
                event_queue.push_back(event);
                last_event_time = Instant::now();
            }
        }

        // If the debounce duration has passed and there are events in the queue,
        // process them and trigger a scan.
        if !event_queue.is_empty() && last_event_time.elapsed() >= DEBOUNCE_DURATION {
            // Clear the queue as we are about to do a full scan.
            event_queue.clear();
            let pool_clone = Arc::clone(&pool);
            let sender_clone = sender.clone();
            let rt_clone = Arc::clone(&rt);

            // Spawn a blocking task for the full scan to avoid blocking the event processor.
            rt_clone.spawn(async move {
                println!("Debounced file system change detected. Starting full scan...");
                run_full_scan(&pool_clone, &sender_clone).await;
            });
        }

        // Sleep for a short duration to avoid busy-waiting.
        sleep(Duration::from_millis(100));
    }
}

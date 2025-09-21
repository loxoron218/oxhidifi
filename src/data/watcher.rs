use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::{Receiver, channel},
    },
    thread::{sleep, spawn},
    time::{Duration, Instant},
};

use notify::{
    Config, Error, ErrorKind, Event,
    EventKind::{Create, Modify, Remove},
    PollWatcher, RecommendedWatcher,
    RecursiveMode::Recursive,
    Watcher,
    event::ModifyKind::{Data, Metadata, Name},
};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::data::{db::query::fetch_all_folders, scanner::library_ops::run_full_scan};

/// The duration to wait after a file system event before triggering a full library scan.
/// This helps to debounce rapid file changes and prevents excessive scanning.
const DEBOUNCE_DURATION: Duration = Duration::from_secs(3);
/// The interval for polling the database for new library folders.
const DB_POLL_INTERVAL: Duration = Duration::from_secs(60);
/// The interval for the file system watcher to scan for changes.
const FS_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Static flag to track if the fallback notification has been shown
static FALLBACK_OCCURRED: AtomicBool = AtomicBool::new(false);

/// Checks if an error is specifically a MaxFilesWatch error that indicates
/// the inotify limit has been reached on Linux.
fn is_max_files_watch_error(error: &Error) -> bool {
    // On Linux, when the inotify watch limit is reached, we typically get
    // an ENOSPC (No space left on device) error with error code 28
    match &error.kind {
        ErrorKind::Io(io_err) => {
            if cfg!(target_os = "linux") {
                // Check for ENOSPC error code (28 on Linux)
                io_err.raw_os_error() == Some(28)
            } else {
                // For other platforms, we might need different handling
                false
            }
        }
        _ => false,
    }
}

/// Factory function to create the appropriate watcher based on system capabilities.
/// First attempts to create a RecommendedWatcher, and falls back to PollWatcher
/// only if MaxFilesWatch error is detected.
fn create_watcher<F>(
    event_handler: F,
    show_notification: bool,
) -> Result<Box<dyn Watcher + Send>, Error>
where
    F: Fn(Result<Event, Error>) + Send + 'static + Clone,
{
    // Configure the watcher with the specified polling interval
    let config = Config::default().with_poll_interval(FS_POLL_INTERVAL);

    // Try to create RecommendedWatcher first (platform-specific, more efficient)
    match RecommendedWatcher::new(event_handler.clone(), config) {
        Ok(watcher) => {
            println!("Using RecommendedWatcher for file system monitoring");
            Ok(Box::new(watcher))
        }
        Err(e) => {
            // Check if it's specifically the MaxFilesWatch error
            if is_max_files_watch_error(&e) {
                eprintln!("Max files watch limit reached, falling back to PollWatcher");

                // Show notification only once per session
                if show_notification && !FALLBACK_OCCURRED.load(Relaxed) {
                    FALLBACK_OCCURRED.store(true, Relaxed);

                    // In a real implementation, we would send a message to the main thread to show the notification
                    println!("Fallback notification should be shown here");
                }

                // Fall back to PollWatcher
                let poll_watcher = PollWatcher::new(event_handler, config)?;
                Ok(Box::new(poll_watcher))
            } else {
                // Propagate other errors as critical failures
                Err(e)
            }
        }
    }
}

/// Spawns new threads for watching library folders and processing file events.
///
/// This function sets up a multi-threaded system for monitoring file system changes.
/// One thread listens for `notify` events and sends them to a channel. A second thread
/// receives these events, debounces them, and triggers a full library scan when the
/// file system has been quiet for a specified duration.
///
/// This implementation first tries to use the platform-specific RecommendedWatcher for
/// optimal performance, and only falls back to PollWatcher if the system's inotify
/// limits are reached.
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

        // Create the file system watcher with the channel sender.
        let mut watcher: Box<dyn Watcher + Send> = match create_watcher(
            move |res| {
                if let Ok(event) = res
                    && let Err(e) = tx.send(event)
                {
                    eprintln!("Failed to send event from watcher: {}", e);
                }
            },
            true,
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create watcher: {}", e);
                return;
            }
        };

        // Initialize the set of watched paths and start the main loop
        // that periodically fetches folders from the database
        let mut watched_paths: HashSet<PathBuf> = HashSet::new();
        loop {
            let folders_to_watch = rt.block_on(async {
                fetch_all_folders(&pool).await.unwrap_or_else(|e| {
                    eprintln!("Error fetching folders for watcher: {}", e);
                    Vec::new()
                })
            });

            // Create a set of current paths from the folders to watch
            let current_paths: HashSet<PathBuf> =
                folders_to_watch.into_iter().map(|f| f.path).collect();

            // Unwatch paths that are no longer in the database
            let paths_to_unwatch = watched_paths.difference(&current_paths);
            for path in paths_to_unwatch {
                if let Err(e) = watcher.unwatch(path) {
                    eprintln!("Warning: could not unwatch path {:?}: {}", path, e);
                }
            }

            // Watch new paths that were added to the database
            let paths_to_watch = current_paths.difference(&watched_paths);
            for path in paths_to_watch {
                if let Err(e) = watcher.watch(path, Recursive) {
                    eprintln!("Failed to watch path {:?}: {}", path, e);
                }
            }

            // Update the state of watched paths for the next iteration
            watched_paths = current_paths;

            // Wait before checking for folder changes again.
            sleep(DB_POLL_INTERVAL);
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
        if let Ok(event) = rx.try_recv()
            && matches!(
                event.kind,
                Create(_) | Remove(_) | Modify(Name(_)) | Modify(Data(_)) | Modify(Metadata(_))
            )
        {
            event_queue.push_back(event);
            last_event_time = Instant::now();
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
        sleep(Duration::from_millis(10));
    }
}

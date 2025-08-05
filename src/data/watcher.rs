use std::{
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode::Recursive, Watcher, event::ModifyKind,
    recommended_watcher,
};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::data::{db::query::fetch_all_folders, scanner::library_ops::run_full_scan};

/// Spawns a new thread that watches the library folders for changes.
///
/// Upon detecting a change, it sends a message through the provided channel
/// to trigger a UI refresh. Includes a debounce mechanism to avoid excessive updates.
///
/// This function initializes a file system watcher for all registered library folders.
/// Any relevant file system events (creation, deletion, modification) within these
/// folders will trigger a debounced full library scan and a UI refresh signal.
///
/// # Arguments
///
/// * `pool` - An `Arc` to the SQLite database pool, used for fetching folder paths
///            and for running the full library scan.
/// * `sender` - An `UnboundedSender` to send a signal to the UI to refresh.
///              This is typically a channel connected to the main application loop.
pub fn start_watching_library(pool: Arc<SqlitePool>, sender: UnboundedSender<()>) {
    // Spawn a new thread to run the file system watcher.
    // This thread will manage its own Tokio runtime for async operations.
    thread::spawn(move || {
        let rt = Runtime::new().expect("Failed to create Tokio runtime for watcher thread");

        // Fetch all library folders from the database that need to be watched.
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

        // The debouncer state. A `JoinHandle` is stored in the `Option`.
        // When a new event arrives, the old timer (if any) is dropped by replacing
        // the `JoinHandle`, effectively cancelling the previous debounced operation.
        let debouncer: Arc<Mutex<Option<thread::JoinHandle<()>>>> = Arc::new(Mutex::new(None));

        // Define the event handler closure for the file system watcher.
        let event_handler = move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // We are interested in events that signify a change in the library's content
                    // (e.g., file creation, deletion, or modification of name/data/metadata).
                    if matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Remove(_)
                            | EventKind::Modify(ModifyKind::Name(_))
                            | EventKind::Modify(ModifyKind::Data(_))
                            | EventKind::Modify(ModifyKind::Metadata(_))
                    ) {
                        // Clone necessary Arcs for the debounced thread.
                        let sender_clone = sender.clone();
                        let debouncer_clone = debouncer.clone();
                        let pool_clone = pool.clone();

                        // Acquire a lock on the debouncer state.
                        let mut guard = debouncer_clone
                            .lock()
                            .expect("Failed to lock debouncer mutex");

                        // Spawn a new thread for the debounced operation.
                        // The previous `JoinHandle` (if any) is dropped here, cancelling the old timer.
                        *guard = Some(thread::spawn(move || {
                            // Wait for a short period to debounce events.
                            // If another relevant event occurs within this duration,
                            // the current timer will be cancelled (by dropping this thread's handle)
                            // and a new one will start.
                            thread::sleep(Duration::from_secs(3));

                            // Create a new Tokio runtime for this specific async operation.
                            // This ensures that `run_full_scan` can execute async code.
                            let rt_inner =
                                Runtime::new().expect("Failed to create Tokio runtime for scan");

                            // Execute the full library scan.
                            rt_inner.block_on(async {
                                run_full_scan(&pool_clone, &sender_clone).await;
                            });
                        }));
                    }
                }
                Err(e) => eprintln!("Watcher error: {}", e),
            }
        };

        // Create a new file system watcher instance with the defined event handler.
        let mut watcher: RecommendedWatcher = match recommended_watcher(event_handler) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create watcher: {}", e);
                return; // Exit the thread if watcher creation fails.
            }
        };

        // Add each configured folder to the watcher for recursive monitoring.
        for folder in folders_to_watch {
            let path = Path::new(&folder.path);
            if let Err(e) = watcher.watch(path, Recursive) {
                eprintln!("Failed to watch path {:?}: {}", path, e);
            }
        }

        // The watcher operates in its own thread, dispatching events.
        // To keep this spawned thread alive indefinitely, we enter a blocking loop.
        // This prevents the `thread::spawn` closure from exiting and dropping the watcher.
        // A more sophisticated shutdown mechanism could be implemented here if needed.
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    });
}

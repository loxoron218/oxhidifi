use std::{path::Path, thread, time::Duration};
use std::sync::{Arc, Mutex};

use notify::{event::ModifyKind, Event, EventKind, RecommendedWatcher, recommended_watcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::data::db::db_query::fetch_all_folders;
use crate::data::scanner::library_ops::run_full_scan;

/// Spawns a new thread that watches the library folders for changes.
/// Upon detecting a change, it sends a message through the provided channel
/// to trigger a UI refresh. Includes a debounce mechanism to avoid excessive updates.
pub fn start_watching_library(pool: Arc<SqlitePool>, sender: UnboundedSender<()>) {
    thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        let folders_to_watch = rt.block_on(async {
            fetch_all_folders(&pool).await.unwrap_or_default()
        });
        if folders_to_watch.is_empty() {
            return;
        }

        // The debouncer state. A new timer is stored in the Option.
        // When a new event arrives, the old timer (if any) is dropped, cancelling it.
        let debouncer = Arc::new(Mutex::new(None::<thread::JoinHandle<()>>));

        // The event handler closure for the watcher.
        let event_handler = move |res: Result<Event, _>| {
            if let Ok(event) = res {

                // We are interested in events that change the content of the library.
                if matches!(
                    event.kind,
                    EventKind::Create(_) |
                    EventKind::Remove(_) |
                    EventKind::Modify(ModifyKind::Name(_)) |
                    EventKind::Modify(ModifyKind::Data(_)) |
                    EventKind::Modify(ModifyKind::Metadata(_))
                ) {

                    // When a relevant event occurs, we trigger the debouncer.
                    let sender_clone = sender.clone();
                    let debouncer_clone = debouncer.clone();
                    let mut guard = debouncer_clone.lock().unwrap();

                    // If there's an existing timer, it will be dropped, cancelling it.
                    let pool_clone = pool.clone();
                    *guard = Some(thread::spawn(move || {

                        // Wait for a short period before sending the signal.
                        // If another event comes in, the old handle will be dropped
                        // and a new timer will start.
                        thread::sleep(Duration::from_secs(3));
                        let rt = Runtime::new().unwrap();
                        rt.block_on(async {
                            run_full_scan(&pool_clone, &sender_clone).await;
                        });
                    }));
                }
            }
        };

        // Create a watcher instance.
        let mut watcher: RecommendedWatcher = match recommended_watcher(event_handler) {
            Ok(w) => w,
            Err(_e) => {
                return;
            }
        };

        // Add each folder to the watcher.
        for folder in folders_to_watch {
            if let Err(_e) = watcher.watch(Path::new(&folder.path), RecursiveMode::Recursive) {
            }
        }

        // The watcher runs in its own thread, so we just need to keep this thread alive.
        // We can just loop to keep the thread from exiting.
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    });
}
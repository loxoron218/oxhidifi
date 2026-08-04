//! Visible queue view with track list, drag-and-drop reorder, and remove button.
//!
//! Displays the playback queue with current/upcoming sections.
//! Uses `ListView` with compact rows. Each row has a drag handle to reorder.
//! Subscribes to `PlaybackEvent` for fully event-driven updates.

use std::{collections::HashMap, sync::Arc};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gio::ListStore,
        glib::{
            BoxedAnyObject, ControlFlow::Break, MainContext, idle_add_local, prelude::StaticType,
        },
        gtk::{Box, ListView, NoSelection, Orientation::Vertical, accessible::Property::Label},
        prelude::{AccessibleExtManual, BoxExt},
    },
    parking_lot::Mutex,
    tokio::spawn,
    tracing::error,
};

use crate::{
    app::AppState,
    playback::{
        control::PlaybackController,
        queue::PlaybackQueue,
        state::PlaybackEvent::{self, QueueChanged, TrackStarted},
    },
    storage::Storage,
    ui::player::queue_row::build_row_factory,
};

/// Data for a single queue entry.
#[derive(Clone, Debug)]
pub struct QueueItemData {
    /// Display name for the track.
    pub name: String,
    /// Whether this is the currently playing track.
    pub is_current: bool,
}

/// Spawn fetching track names in a background thread.
fn spawn_fetch_queue_names(state: &Arc<AppState>, ids: Vec<i64>, tx: Sender<Vec<(i64, String)>>) {
    let s = Arc::clone(state);
    spawn(async move {
        let storage = &s.storage;
        let tracks = storage.get_tracks_by_ids(&ids).await.unwrap_or_default();
        let track_map: HashMap<i64, String> = tracks.into_iter().map(|t| (t.id, t.title)).collect();
        let names: Vec<(i64, String)> = ids
            .iter()
            .map(|id| {
                let name = track_map
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("Track #{id}"));
                (*id, name)
            })
            .collect();
        if let Err(e) = tx.try_send(names) {
            error!(error = %e, "Failed to send queue names");
        }
    });
}

/// Refresh the store from the main thread via `idle_add_local`.
fn refresh_store_on_main(store: &ListStore, queue: &PlaybackQueue, names: &[(i64, String)]) {
    let s = store.clone();
    let q = queue.clone();
    let n = names.to_vec();
    idle_add_local(move || {
        populate_store(&s, &q, &n);
        Break
    });
}

/// Handle a single playback event for queue updates.
fn handle_queue_event(
    event: PlaybackEvent,
    state: &Arc<AppState>,
    store: &ListStore,
    queue: &PlaybackQueue,
    cache: &Arc<Mutex<Vec<(i64, String)>>>,
    tx: &Sender<Vec<(i64, String)>>,
) {
    match event {
        QueueChanged { track_ids } => {
            refresh_store_on_main(store, queue, &cache.lock());
            spawn_fetch_queue_names(state, track_ids, tx.clone());
        }
        TrackStarted { .. } => {
            refresh_store_on_main(store, queue, &cache.lock());
        }
        _ => {}
    }
}

/// Update the name cache from received queue names.
fn update_name_cache(cache: &Arc<Mutex<Vec<(i64, String)>>>, names: &Vec<(i64, String)>) {
    cache.lock().clone_from(names);
}

/// Build the queue view using `ListView` with compact rows.
///
/// Each row has a drag handle for reordering, track name, and remove button.
#[must_use]
pub fn build_queue_view(state: &Arc<AppState>, queue: &PlaybackQueue) -> Box {
    let store = ListStore::builder()
        .item_type(BoxedAnyObject::static_type())
        .build();

    let queue = queue.clone();
    let rx = state.playback.subscribe();

    let factory = build_row_factory(&queue, &store);

    let model = NoSelection::new(Some(store.clone()));
    let list_view = ListView::builder()
        .model(&model)
        .factory(&factory)
        .single_click_activate(false)
        .show_separators(true)
        .can_focus(true)
        .build();
    list_view.update_property(&[Label("Playback queue list")]);

    let container = Box::builder().orientation(Vertical).spacing(4).build();

    container.append(&list_view);

    let poll_state = Arc::clone(state);
    let poll_store = store;
    let poll_queue = queue;

    let (queue_tx, queue_rx) = unbounded::<Vec<(i64, String)>>();
    let cached_names = Arc::new(Mutex::new(Vec::<(i64, String)>::new()));

    let ev_state = Arc::clone(&poll_state);
    let ev_store = poll_store.clone();
    let ev_queue = poll_queue.clone();
    let ev_tx = queue_tx;
    let ev_cache = Arc::clone(&cached_names);
    MainContext::default().spawn_local(async move {
        while let Ok(event) = rx.recv().await {
            handle_queue_event(event, &ev_state, &ev_store, &ev_queue, &ev_cache, &ev_tx);
        }
    });

    let names_store = poll_store;
    let names_queue = poll_queue;
    let names_cache = cached_names;
    MainContext::default().spawn_local(async move {
        while let Ok(names) = queue_rx.recv().await {
            update_name_cache(&names_cache, &names);
            refresh_store_on_main(&names_store, &names_queue, &names);
        }
    });

    container
}

/// Populate the `ListStore` with current queue data.
pub fn populate_store(store: &ListStore, queue: &PlaybackQueue, name_cache: &[(i64, String)]) {
    store.remove_all();

    let current_id = queue.current();
    let tracks = queue.tracks();

    for &track_id in &tracks {
        let is_current = current_id == Some(track_id);
        let name = name_cache
            .iter()
            .find(|(id, _)| *id == track_id)
            .map_or_else(|| format!("Track #{track_id}"), |(_, n)| n.clone());

        let data = QueueItemData { name, is_current };
        let boxed = BoxedAnyObject::new(data);
        store.append(&boxed);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gio::{ListStore, prelude::ListModelExt},
            glib::{BoxedAnyObject, object::Cast, prelude::StaticType},
            gtk::{self, test},
        },
        parking_lot::Mutex,
    };

    use crate::{
        playback::queue::PlaybackQueue,
        ui::player::queue::{QueueItemData, populate_store, update_name_cache},
    };

    fn make_store() -> ListStore {
        ListStore::builder()
            .item_type(BoxedAnyObject::static_type())
            .build()
    }

    fn item_data(store: &ListStore, index: u32) -> Option<(String, bool)> {
        let item = store.item(index)?;
        let Ok(boxed) = item.downcast::<BoxedAnyObject>() else {
            return None;
        };
        let data = boxed.borrow::<QueueItemData>();
        Some((data.name.clone(), data.is_current))
    }

    #[test]
    fn queue_starts_empty() {
        let q = PlaybackQueue::new();
        assert!(q.is_empty());
    }

    #[test]
    fn queue_tracks_returns_ids() {
        let q = PlaybackQueue::new();
        q.set_queue(vec![1, 2, 3]);
        assert_eq!(q.tracks(), vec![1, 2, 3]);
    }

    #[test]
    fn update_name_cache_replaces_entries() -> Result<()> {
        let cache = Arc::new(Mutex::new(vec![(1, "Old".to_string())]));
        let names = vec![(2, "New".to_string())];
        update_name_cache(&cache, &names);
        ensure!(*cache.lock() == vec![(2, "New".to_string())]);
        Ok(())
    }

    #[test]
    fn populate_store_sets_names_and_current() -> Result<()> {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![10, 20]);
        queue.set_current_index(1);
        let store = make_store();
        let names = vec![(10, "Alpha".to_string()), (20, "Beta".to_string())];
        populate_store(&store, &queue, &names);
        ensure!(store.n_items() == 2);
        ensure!(item_data(&store, 0) == Some(("Alpha".to_string(), false)));
        ensure!(item_data(&store, 1) == Some(("Beta".to_string(), true)));
        Ok(())
    }

    #[test]
    fn populate_store_falls_back_to_track_id() -> Result<()> {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![42]);
        let store = make_store();
        populate_store(&store, &queue, &[]);
        ensure!(item_data(&store, 0) == Some(("Track #42".to_string(), true)));
        Ok(())
    }
}

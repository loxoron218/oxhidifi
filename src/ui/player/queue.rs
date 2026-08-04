//! Visible queue view with track list, drag-and-drop reorder, and remove button.
//!
//! Displays the playback queue with current/upcoming sections.
//! Uses `ListView` with compact rows. Each row has a drag handle to reorder.
//! Subscribes to `PlaybackEvent` for fully event-driven updates.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gdk::{ContentProvider, DragAction},
        gio::{ListStore, prelude::ListModelExt},
        glib::{
            BoxedAnyObject, ControlFlow::Break, MainContext, Value, idle_add_local,
            prelude::StaticType, types::Type, value::ToValue,
        },
        gtk::{
            Align::Start,
            Box, Button, DragSource, DropTarget, Label, ListItem, ListView, NoSelection,
            Orientation::{Horizontal, Vertical},
            SignalListItemFactory,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, ListItemExt, WidgetExt},
    },
    tokio::spawn,
    tracing::{error, warn},
};

use crate::{
    app::AppState,
    playback::{
        control::PlaybackController,
        queue::PlaybackQueue,
        state::PlaybackEvent::{self, QueueChanged, TrackStarted},
    },
    storage::Storage,
};

/// Data for a single queue entry.
#[derive(Clone, Debug)]
struct QueueItemData {
    /// Display name for the track.
    name: String,
    /// Whether this is the currently playing track.
    is_current: bool,
}

/// Reorder an item within both the queue model and the `ListStore`.
fn reorder_entry(queue: &PlaybackQueue, store: &ListStore, from: usize, to: usize) {
    if from == to {
        return;
    }
    queue.move_track(from, to);
    let Ok(from_u32) = u32::try_from(from) else {
        return;
    };
    let Some(item) = store.item(from_u32) else {
        return;
    };
    store.remove(from_u32);
    let adjusted_pos = if to > from { to - 1 } else { to };
    let Ok(adjusted_u32) = u32::try_from(adjusted_pos) else {
        return;
    };
    store.insert(adjusted_u32, &item);
}

/// Process a drop value for reordering.
fn handle_drop_value(value: &Value, queue: &PlaybackQueue, store: &ListStore, to_pos: usize) {
    let from = match value.get::<i32>() {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "Failed to get drop value");
            return;
        }
    };
    let from_u = usize::try_from(from).unwrap_or(0);
    reorder_entry(queue, store, from_u, to_pos);
}

/// Remove a track from the queue at the given position, updating the store.
/// Logs a warning if the position is out of bounds.
fn try_remove_entry(q: &PlaybackQueue, store: &ListStore, pos: usize) {
    store.remove(u32::try_from(pos).unwrap_or(0));
    if q.remove(pos).is_none() {
        warn!(pos, "Failed to remove track — position out of bounds");
    }
}

/// Create the `SignalListItemFactory` that builds and binds queue rows.
fn build_row_factory(queue: &PlaybackQueue, store: &ListStore) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let factory_queue = queue.clone();
    let factory_store = store.clone();

    factory.connect_setup(move |_, list_item| {
        let Some(list_item_obj) = list_item.downcast_ref::<ListItem>() else {
            return;
        };
        let li = list_item_obj.clone();
        let queue_li = factory_queue.clone();
        let store_li = factory_store.clone();

        let container = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .margin_top(3)
            .margin_bottom(3)
            .margin_start(6)
            .margin_end(6)
            .build();

        let handle = Button::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Drag to reorder")
            .can_focus(true)
            .build();
        handle.update_property(&[PropertyLabel("Drag handle")]);

        let drag = DragSource::builder().actions(DragAction::MOVE).build();
        let li_drag = li.clone();
        drag.connect_prepare(move |_, _, _| {
            let pos = li_drag.position();
            let pos_i32 = i32::try_from(pos).unwrap_or(0);
            let value = pos_i32.to_value();
            Some(ContentProvider::for_value(&value))
        });
        handle.add_controller(drag);

        let label = Label::builder()
            .ellipsize(End)
            .max_width_chars(25)
            .halign(Start)
            .hexpand(true)
            .build();
        label.update_property(&[PropertyLabel("Track name in queue")]);

        let remove = Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Remove from queue")
            .can_focus(true)
            .build();
        remove.update_property(&[PropertyLabel("Remove from queue")]);

        let li_remove = li.clone();
        let queue_remove = queue_li.clone();
        let store_remove = store_li.clone();
        remove.connect_clicked(move |_| {
            let pos = li_remove.position() as usize;
            try_remove_entry(&queue_remove, &store_remove, pos);
        });

        let drop = DropTarget::new(Type::I32, DragAction::MOVE);
        let li_drop = li;
        let queue_drop = queue_li;
        let store_drop = store_li;
        drop.connect_drop(move |_, value, _, _| {
            let to_pos = li_drop.position() as usize;
            handle_drop_value(value, &queue_drop, &store_drop, to_pos);
            true
        });

        container.append(&handle);
        container.append(&label);
        container.append(&remove);
        container.add_controller(drop);

        list_item_obj.set_child(Some(&container));
    });

    factory.connect_bind(|_, list_item| {
        let Some(list_item) = list_item.downcast_ref::<ListItem>() else {
            return;
        };
        bind_row(list_item);
    });

    factory
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
            refresh_store_on_main(
                store,
                queue,
                &cache.lock().unwrap_or_else(PoisonError::into_inner),
            );
            spawn_fetch_queue_names(state, track_ids, tx.clone());
        }
        TrackStarted { .. } => {
            if let Ok(guard) = cache.lock() {
                refresh_store_on_main(store, queue, &guard);
            }
        }
        _ => {}
    }
}

/// Update the name cache from received queue names.
fn update_name_cache(cache: &Arc<Mutex<Vec<(i64, String)>>>, names: &Vec<(i64, String)>) {
    if let Ok(mut guard) = cache.lock() {
        guard.clone_from(names);
    }
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
    list_view.update_property(&[PropertyLabel("Playback queue list")]);

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
fn populate_store(store: &ListStore, queue: &PlaybackQueue, name_cache: &[(i64, String)]) {
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

/// Bind data to a row widget (called when item data changes).
fn bind_row(list_item: &ListItem) {
    let Some(item) = list_item.item() else {
        return;
    };
    let Some(boxed) = item.downcast_ref::<BoxedAnyObject>() else {
        return;
    };
    let data = boxed.borrow::<QueueItemData>();

    let Some(child) = list_item.child() else {
        return;
    };
    let Some(container) = child.downcast_ref::<Box>() else {
        return;
    };
    let Some(handle) = container.first_child() else {
        return;
    };
    let Some(label_widget) = handle.next_sibling() else {
        return;
    };
    let Some(label) = label_widget.downcast_ref::<Label>() else {
        return;
    };

    label.set_label(&data.name);

    let mut classes: Vec<&str> = vec![];
    if data.is_current {
        classes.push("heading");
    }
    label.set_css_classes(&classes);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gio::{ListStore, prelude::ListModelExt},
            glib::{BoxedAnyObject, object::Cast, prelude::StaticType},
            gtk::{self, test},
        },
    };

    use crate::{
        playback::queue::PlaybackQueue,
        ui::player::queue::{QueueItemData, populate_store, try_remove_entry, update_name_cache},
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

    fn queue_and_store() -> (PlaybackQueue, ListStore) {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![10, 20]);
        let store = make_store();
        let names = vec![(10, "Alpha".to_string()), (20, "Beta".to_string())];
        populate_store(&store, &queue, &names);
        (queue, store)
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
        let replaced = {
            let guard = cache.lock().unwrap_or_else(PoisonError::into_inner);
            *guard == vec![(2, "New".to_string())]
        };
        ensure!(replaced);
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

    #[test]
    fn try_remove_entry_removes_in_bounds() -> Result<()> {
        let (queue, store) = queue_and_store();
        try_remove_entry(&queue, &store, 0);
        ensure!(store.n_items() == 1);
        ensure!(queue.len() == 1);
        ensure!(queue.current() == Some(20));
        Ok(())
    }

    #[test]
    fn try_remove_entry_out_of_bounds_is_noop() -> Result<()> {
        let (queue, store) = queue_and_store();
        try_remove_entry(&queue, &store, 5);
        ensure!(store.n_items() == 2);
        ensure!(queue.len() == 2);
        Ok(())
    }
}

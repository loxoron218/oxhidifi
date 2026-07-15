//! Visible queue view with track list, drag-and-drop reorder, and remove button.
//!
//! Displays the playback queue with current/upcoming sections.
//! Uses `ListView` with compact rows. Each row has a drag handle to reorder.
//! Subscribes to `PlaybackEvent` for fully event-driven updates.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
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
    tracing::error,
};

use crate::{
    app::AppState,
    playback::{
        engine::{
            PlaybackController,
            PlaybackEvent::{self, QueueChanged, TrackStarted},
        },
        queue::PlaybackQueue,
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

/// Create the `SignalListItemFactory` that builds and binds queue rows.
fn build_row_factory(queue: &PlaybackQueue, store: &ListStore) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let factory_queue = queue.clone();
    let factory_store = store.clone();

    factory.connect_setup(move |_factory, list_item| {
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
            .build();
        handle.update_property(&[PropertyLabel("Drag handle")]);

        let drag = DragSource::builder().actions(DragAction::MOVE).build();
        let li_drag = li.clone();
        drag.connect_prepare(move |_src, _x, _y| {
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
            .build();
        remove.update_property(&[PropertyLabel("Remove from queue")]);

        let li_remove = li.clone();
        let queue_remove = queue_li.clone();
        let store_remove = store_li.clone();
        remove.connect_clicked(move |_| {
            let pos = li_remove.position() as usize;
            let q = queue_remove.clone();
            let _entry = q.remove(pos);
            store_remove.remove(u32::try_from(pos).unwrap_or(0));
        });

        let drop = DropTarget::new(Type::I32, DragAction::MOVE);
        let li_drop = li;
        let queue_drop = queue_li;
        let store_drop = store_li;
        drop.connect_drop(move |_target, value, _x, _y| {
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

    factory.connect_bind(|_factory, list_item| {
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
        if tx.try_send(names).is_err() {
            error!(target: "ui::player::queue", "Failed to send queue names");
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
    use crate::playback::queue::PlaybackQueue;

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
}

//! Visible queue view with track list, drag-and-drop reorder, and remove button.
//!
//! Displays the playback queue with current/upcoming sections.
//! Uses `ListView` with compact rows. Each row has a drag handle to reorder.

use std::sync::Arc;

use {
    libadwaita::{
        gdk::{ContentProvider, DragAction},
        gio::{ListStore, prelude::ListModelExt},
        glib::{
            BoxedAnyObject, ControlFlow::Break, Value, idle_add_local, prelude::StaticType,
            spawn_future_local, types::Type, value::ToValue,
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
    tracing::error,
};

use crate::{
    app::AppState,
    playback::{engine::PlaybackController, queue::PlaybackQueue},
    storage::Storage,
};

/// Data for a single queue entry.
#[derive(Clone, Debug)]
struct QueueItemData {
    /// Track ID from storage.
    track_id: i64,
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

/// Refresh the store on a queue change event.
fn on_queue_event(state: &Arc<AppState>, store: &ListStore, queue: &PlaybackQueue) {
    let state_c = Arc::clone(state);
    let store_c = store.clone();
    let queue_c = queue.clone();
    spawn_future_local(async move {
        let ids = queue_c.tracks();
        let names = fetch_track_names(&state_c, &ids).await;
        let s = store_c;
        let q = queue_c;
        idle_add_local(move || {
            populate_store(&s, &q, &names);
            Break
        });
    });
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

/// Build the queue view using `ListView` with compact rows.
///
/// Each row has a drag handle for reordering, track name, and remove button.
#[must_use]
pub fn build_queue_view(state: &Arc<AppState>, queue: &PlaybackQueue) -> Box {
    let store = ListStore::builder()
        .item_type(BoxedAnyObject::static_type())
        .build();

    let queue = queue.clone();
    let state_arc = Arc::clone(state);

    on_queue_event(&state_arc, &store, &queue);

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

    let state_evt = Arc::clone(state);
    let store_evt = store;
    let q_evt = queue;
    spawn_future_local(async move {
        let mut event_rx = state_evt.playback.subscribe();
        while event_rx.recv().await.is_ok() {
            let ids = q_evt.tracks();
            let names = fetch_track_names(&state_evt, &ids).await;
            refresh_store_on_main(&store_evt, &q_evt, &names);
        }
    });

    container
}

/// Fetch display names for all track IDs concurrently.
async fn fetch_track_names(state: &Arc<AppState>, ids: &[i64]) -> Vec<(i64, String)> {
    let storage = &state.storage;
    let mut names = Vec::with_capacity(ids.len());
    for &id in ids {
        let name = match storage.get_track(id).await {
            Ok(Some(track)) => track.title,
            Ok(None) | Err(_) => format!("Track #{id}"),
        };
        names.push((id, name));
    }
    names
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

        let data = QueueItemData {
            track_id,
            name,
            is_current,
        };
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

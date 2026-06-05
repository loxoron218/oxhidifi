//! Visible queue view with track list, drag-and-drop reorder, and remove button.
//!
//! Displays the playback queue with current/upcoming sections.
//! Supports drag-and-drop reorder via `GtkDragSource`/`GtkDropTarget`.

use std::sync::Arc;

use {
    libadwaita::{
        gdk::{ContentProvider, DragAction},
        glib::{
            ControlFlow::Break,
            idle_add_local, spawn_future_local,
            types::Type as GType,
            value::{ToValue, Value},
        },
        gtk::{
            Align::{Center, Start},
            Box, Button, DragSource, DropTarget, Image, Label, ListBox, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            ScrolledWindow,
            SelectionMode::None as SelectionNone,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::AppState,
    playback::{engine::PlaybackController, queue::PlaybackQueue},
    storage::Storage,
};

/// Build the queue view.
///
/// Creates a `ScrolledWindow` containing a `ListBox` populated with
/// queue entries. Shows current/upcoming sections.
#[must_use]
pub fn build_queue_view(state: &Arc<AppState>, queue: &PlaybackQueue) -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let list = ListBox::builder()
        .selection_mode(SelectionNone)
        .can_focus(true)
        .tooltip_text("Playback queue — drag to reorder, click remove to delete")
        .css_classes(["boxed-list"])
        .build();

    let initial_queue = queue.clone();
    let initial_state = Arc::clone(state);
    let initial_list = list.clone();
    let initial_ids = initial_queue.tracks();
    spawn_future_local(async move {
        let names = fetch_track_names(&initial_state, &initial_ids).await;
        idle_add_local(move || {
            populate_queue_list(&initial_list, &initial_queue, &initial_state, &names);
            Break
        });
    });

    scrolled.set_child(Some(&list));

    let mut event_rx = state.playback.subscribe();
    let list_ref = list;
    let state_ref = Arc::clone(state);
    let queue_ref = queue.clone();
    spawn_future_local(async move {
        while event_rx.recv().await.is_ok() {
            on_queue_event(&state_ref, &queue_ref, &list_ref).await;
        }
    });

    scrolled
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

/// Remove all children from a list box.
fn clear_container(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

/// Handle a queue change event by re-fetching names and repopulating.
async fn on_queue_event(state: &Arc<AppState>, queue: &PlaybackQueue, list: &ListBox) {
    let q = queue.clone();
    let names = fetch_track_names(state, &q.tracks()).await;
    let l = list.clone();
    let s = Arc::clone(state);
    idle_add_local(move || {
        populate_queue_list(&l, &q, &s, &names);
        Break
    });
}

/// Populate the queue list with pre-fetched track names.
fn populate_queue_list(
    list: &ListBox,
    queue: &PlaybackQueue,
    state: &Arc<AppState>,
    name_cache: &[(i64, String)],
) {
    clear_container(list);

    let current_id = queue.current();
    let tracks = queue.tracks();

    if tracks.is_empty() {
        let empty_label = Label::builder()
            .label("Queue is empty")
            .css_classes(["dim-label", "body"])
            .vexpand(true)
            .valign(Center)
            .build();
        empty_label.update_property(&[PropertyLabel("Queue is empty")]);
        list.append(&empty_label);
        return;
    }

    let current_index = current_id.and_then(|cid| tracks.iter().position(|&id| id == cid));

    for (i, &track_id) in tracks.iter().enumerate() {
        let is_current = current_index == Some(i);
        let track_name = name_cache
            .iter()
            .find(|(id, _)| *id == track_id)
            .map_or_else(|| format!("Track #{track_id}"), |(_, name)| name.clone());
        let row = build_queue_row(state, list, track_id, &track_name, Some(i), is_current);
        list.append(&row);
    }
}

/// Re-fetch track names and re-populate the queue list asynchronously.
fn reload_queue(state: &Arc<AppState>, list: &ListBox, queue: PlaybackQueue) {
    let state_fut = Arc::clone(state);
    let list_fut = list.clone();
    let q_fut = queue;
    spawn_future_local(async move {
        let tracks = q_fut.tracks();
        let names = fetch_track_names(&state_fut, &tracks).await;
        idle_add_local(move || {
            populate_queue_list(&list_fut, &q_fut, &state_fut, &names);
            Break
        });
    });
}

/// Handle a remove-from-queue button click.
fn on_remove_clicked(list: &ListBox, state: &Arc<AppState>, pos: usize) {
    let q = state.playback.queue().clone();
    let _removed = q.remove(pos);
    clear_container(list);
    reload_queue(state, list, q);
}

/// Handle a drag-and-drop reorder of a queue item.
fn on_drop(list: &ListBox, state: &Arc<AppState>, pos: usize, from_pos: Option<i32>) -> bool {
    let Some(from_pos) = from_pos else {
        return true;
    };
    let from = usize::try_from(from_pos).unwrap_or(0);
    if from != pos {
        let q = state.playback.queue().clone();
        q.move_track(from, pos);
        clear_container(list);
        reload_queue(state, list, q);
    }
    true
}

/// Extract drop position from a GTK `Value` and handle the drop.
fn on_drop_value(list: &ListBox, state: &Arc<AppState>, pos: usize, value: &Value) -> bool {
    let from_pos = match value.get::<i32>() {
        Ok(v) => Some(v),
        Err(e) => {
            error!(error = %e, "Failed to get drop target value");
            None
        }
    };
    on_drop(list, state, pos, from_pos)
}

/// Build a single queue row with track info and remove button.
fn build_queue_row(
    state: &Arc<AppState>,
    list: &ListBox,
    _track_id: i64,
    track_name: &str,
    position: Option<usize>,
    is_current: bool,
) -> ListBoxRow {
    let row_content = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();

    if is_current {
        let now_playing = Image::builder()
            .icon_name("audio-volume-high-symbolic")
            .pixel_size(16)
            .build();
        now_playing.update_property(&[PropertyLabel("Now playing")]);
        row_content.append(&now_playing);
    }

    let info_box = Box::builder()
        .orientation(Vertical)
        .spacing(2)
        .hexpand(true)
        .build();

    let mut title_classes = Vec::new();
    if is_current {
        title_classes.push("heading");
    }
    let title_label = Label::builder()
        .label(track_name)
        .ellipsize(End)
        .max_width_chars(30)
        .halign(Start)
        .css_classes(&*title_classes)
        .build();
    info_box.append(&title_label);

    row_content.append(&info_box);

    let remove_button = Button::builder()
        .icon_name("edit-delete-symbolic")
        .css_classes(["flat", "circular"])
        .valign(Center)
        .tooltip_text("Remove from queue")
        .build();
    remove_button.update_property(&[PropertyLabel("Remove from queue")]);
    row_content.append(&remove_button);

    if let Some(pos) = position {
        let list_for_remove = list.clone();
        let state_for_remove = Arc::clone(state);
        remove_button.connect_clicked(move |_| {
            on_remove_clicked(&list_for_remove, &state_for_remove, pos);
        });
    }

    let row = ListBoxRow::builder()
        .child(&row_content)
        .activatable(false)
        .build();

    if let Some(pos) = position {
        let pos_i32 = i32::try_from(pos).unwrap_or(0);
        let drag_src = DragSource::builder().actions(DragAction::MOVE).build();
        drag_src.connect_prepare(move |_src, _x, _y| {
            let value = pos_i32.to_value();
            Some(ContentProvider::for_value(&value))
        });
        row_content.add_controller(drag_src);

        let drop_target = DropTarget::new(GType::I32, DragAction::MOVE);
        let list_for_dnd = list.clone();
        let state_for_dnd = Arc::clone(state);
        drop_target.connect_drop(move |_target, value, _x, _y| {
            on_drop_value(&list_for_dnd, &state_for_dnd, pos, value)
        });
        row_content.add_controller(drop_target);
    }

    row
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

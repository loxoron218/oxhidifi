//! `GLib` main-context dispatch for the sort/zoom coalescing loop.

use std::sync::Arc;

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::glib::{spawn_future_local, timeout_future},
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::database::SqliteStorage,
    ui::gallery::rebuild_debounce::{
        SORT_ZOOM_DEBOUNCE,
        SortZoomEvent::{self, Preview, Rebuild},
        listen_sort_zoom_loop,
    },
};

/// Spawn a future that listens for sort-config and zoom changes and rebuilds the grid.
///
/// Subscribes to the given per-entity sort receiver and zoom receiver,
/// coalescing rapid changes with the [`SORT_ZOOM_DEBOUNCE`] window. The zoom
/// receiver must be this grid's own channel (see [`AppState::albums_zoom_rx`]
/// / [`AppState::artists_zoom_rx`]) — `async_channel` receivers compete for
/// messages, so sharing one channel across grids would drop every other zoom
/// notification for whichever grid lost the race. The debounce is driven by a
/// glib-native timer future so the whole loop stays on the `GLib` main context
/// without depending on tokio wakers. `preview_zoom` runs immediately on a
/// zoom-only change so cards resize before the debounce; the rebuild handler
/// receives `(sort_fired, zoom_fired)` flags so it can resize in place for
/// zoom-only changes.
///
/// The coalescing loop runs as a `Send` future that emits [`SortZoomEvent`]s
/// over an unbounded channel; a second local consumer performs the actual
/// widget work, keeping non-`Send` widget handles out of the loop.
pub fn spawn_listen_sort_zoom(
    state: &Arc<AppState>,
    sort_rx: Receiver<()>,
    zoom_rx: Receiver<()>,
    preview_zoom: impl Fn() + 'static,
    rebuild: impl Fn(bool, bool) + 'static,
) {
    let storage = Arc::clone(&state.storage);
    let (event_tx, event_rx) = unbounded::<SortZoomEvent>();
    let preview_event_tx = event_tx.clone();
    let rebuild_event_tx = event_tx;

    spawn_future_local(async move {
        listen_sort_zoom_loop(
            sort_rx,
            zoom_rx,
            async || {
                timeout_future(SORT_ZOOM_DEBOUNCE).await;
            },
            move || send_preview_event(&preview_event_tx),
            move |sort_fired, zoom_fired| {
                send_rebuild_event(&rebuild_event_tx, sort_fired, zoom_fired);
            },
        )
        .await;
    });

    spawn_future_local(async move {
        while let Ok(event) = event_rx.recv().await {
            consume_sort_zoom_event(event, &preview_zoom, &rebuild, &storage);
        }
    });
}

/// Apply a coalesced sort/zoom event to the grid widgets.
fn consume_sort_zoom_event(
    event: SortZoomEvent,
    preview_zoom: &impl Fn(),
    rebuild: &impl Fn(bool, bool),
    storage: &Arc<SqliteStorage>,
) {
    match event {
        Preview => preview_zoom(),
        Rebuild(sort_fired, zoom_fired) => {
            rebuild(sort_fired, zoom_fired);
            storage.save_settings();
        }
    }
}

/// Dispatch a preview event from the coalescing loop to the widget consumer.
fn send_preview_event(tx: &Sender<SortZoomEvent>) {
    if let Err(e) = tx.try_send(Preview) {
        warn!(error = %e, "Failed to dispatch sort/zoom preview event");
    }
}

/// Dispatch a rebuild event from the coalescing loop to the widget consumer.
fn send_rebuild_event(tx: &Sender<SortZoomEvent>, sort_fired: bool, zoom_fired: bool) {
    if let Err(e) = tx.try_send(Rebuild(sort_fired, zoom_fired)) {
        warn!(error = %e, "Failed to dispatch sort/zoom rebuild event");
    }
}

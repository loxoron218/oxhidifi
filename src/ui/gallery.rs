//! Library views: album grid/column, artist grid/column, empty state,
//! `GObject` boxed data, and `GtkColumnView` builders.

pub mod album_grid;
pub mod artist_grid;
pub mod avatar;
pub mod boxed_data;
pub mod card;
pub mod coalescer;
pub mod columns;
pub mod empty;
pub mod folder_picker;
pub mod frame_resize;
pub mod grid_flow;
pub mod keyboard_nav;
pub mod label_sizing;
pub mod narrow_flag;
pub mod order_memo;
pub mod play_action;
pub mod precedence;
pub mod priority;
pub mod rebuild_debounce;
pub mod stack_cache;
pub mod table;

use std::sync::Arc;

use libadwaita::{glib::spawn_future_local, gtk::GestureClick};

use crate::app::runtime::{AppState, NavigationEvent};

/// Build a click gesture that sends a navigation event.
///
/// # Arguments
///
/// * `state` - Application state used to dispatch the event
/// * `event` - Navigation event sent on click release
///
/// # Returns
///
/// A `GestureClick` wired to dispatch `event` without blocking the UI.
#[must_use]
pub fn build_navigation_gesture(state: &Arc<AppState>, event: NavigationEvent) -> GestureClick {
    let gesture = GestureClick::new();
    let state_clone = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(gesture.connect_released(move |_, _, _, _| {
            let state_cb = Arc::clone(&state_clone);
            state_clone
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    state_cb.send_navigation_event(event).await;
                }));
        }));
    gesture
}

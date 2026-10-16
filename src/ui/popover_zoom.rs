//! Zoom controls and level handling for the view-toggle popover.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    libadwaita::{
        glib::spawn_future_local,
        gtk::{
            Align::End,
            Box, Button, Label,
            Orientation::{Horizontal, Vertical},
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::view_mode::ViewMode::{self, Column, Grid},
    ui::{
        zoom::{GRID_ZOOM_MAX, LIST_ZOOM_MAX},
        zoom_buttons::update_zoom_sensitivity,
    },
};

/// Build the zoom container with icon-size label and zoom buttons.
///
/// # Arguments
///
/// * `state` - Application state for initial zoom button sensitivity.
///
/// # Returns
///
/// Zoom container with its zoom-out and zoom-in buttons for handler wiring.
pub fn build_zoom_box(state: &Arc<AppState>) -> (Box, Button, Button) {
    let zoom_box = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(6)
        .css_classes(["popover-box"])
        .tooltip_text("View options")
        .build();
    zoom_box.update_property(&[PropertyLabel("View options")]);

    let zoom_controls_box = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .css_classes(["zoom-controls"])
        .tooltip_text("Zoom controls")
        .build();
    let icon_size_label = Label::builder()
        .label("Icon Size")
        .css_classes(["caption"])
        .tooltip_text("Icon size")
        .build();
    zoom_controls_box.append(&icon_size_label);

    let zoom_buttons_box = Box::builder()
        .orientation(Horizontal)
        .css_classes(["linked", "flat"])
        .hexpand(true)
        .halign(End)
        .tooltip_text("Zoom buttons")
        .build();

    let zoom_out_btn = Button::builder()
        .icon_name("zoom-out-symbolic")
        .tooltip_text("Zoom Out")
        .css_classes(["flat", "image-button"])
        .build();

    let zoom_in_btn = Button::builder()
        .icon_name("zoom-in-symbolic")
        .tooltip_text("Zoom In")
        .css_classes(["flat", "image-button"])
        .build();

    update_zoom_sensitivity(state, &zoom_out_btn, &zoom_in_btn);

    zoom_buttons_box.append(&zoom_out_btn);
    zoom_buttons_box.append(&zoom_in_btn);
    zoom_controls_box.append(&zoom_buttons_box);
    zoom_box.append(&zoom_controls_box);
    (zoom_box, zoom_out_btn, zoom_in_btn)
}

/// Decrease the zoom level for the current view mode.
///
/// Only updates in-memory settings; the debounced disk write happens once
/// per zoom episode in `spawn_listen_sort_zoom` (see `common.rs`), so rapid
/// clicking never spawns a write task per click.
///
/// # Returns
///
/// `true` when the level actually changed, `false` at the minimum.
#[must_use]
pub fn apply_zoom_out(state: &AppState, mode: ViewMode) -> bool {
    match mode {
        Grid => {
            let current = state.storage.get_grid_zoom_level();
            let level = current.saturating_sub(1);
            state.storage.set_grid_zoom_level_memory(level);
            level != current
        }
        Column => {
            let current = state.storage.get_list_zoom_level();
            let level = current.saturating_sub(1);
            state.storage.set_list_zoom_level_memory(level);
            level != current
        }
    }
}

/// Increase the zoom level for the current view mode.
///
/// Only updates in-memory settings; the debounced disk write happens once
/// per zoom episode in `spawn_listen_sort_zoom` (see `common.rs`).
///
/// # Returns
///
/// `true` when the level actually changed, `false` at the maximum.
#[must_use]
pub fn apply_zoom_in(state: &AppState, mode: ViewMode) -> bool {
    match mode {
        Grid => {
            let current = state.storage.get_grid_zoom_level();
            let level = current.saturating_add(1).min(GRID_ZOOM_MAX);
            state.storage.set_grid_zoom_level_memory(level);
            level != current
        }
        Column => {
            let current = state.storage.get_list_zoom_level();
            let level = current.saturating_add(1).min(LIST_ZOOM_MAX);
            state.storage.set_list_zoom_level_memory(level);
            level != current
        }
    }
}

/// Send a zoom change notification to every library grid.
///
/// `async_channel` receivers compete for messages rather than broadcasting,
/// so each grid subscribes to its own channel and every zoom change must be
/// fanned out to all of them — otherwise the grid not currently active would
/// silently miss (and drop) every other notification.
///
/// Bumps both grids' `zoom_seq` so superseded batched resize traversals bail
/// via their staleness predicate instead of fighting the debounced resize.
pub fn notify_zoom_change(state: &AppState) {
    _ = state.album_grid.zoom_seq.fetch_add(1, Relaxed);
    _ = state.artist_grid.zoom_seq.fetch_add(1, Relaxed);
    for tx in [&state.albums_zoom_tx, &state.artists_zoom_tx] {
        if let Err(e) = tx.try_send(()) {
            warn!(error = %e, "Failed to send zoom change");
        }
    }
}

/// Connect zoom button click handlers and view-mode sensitivity sync.
///
/// Click handlers apply the zoom for the mode active at press time, fan out
/// only on real changes, and refresh sensitivity with local button clones.
/// A view-mode subscription keeps the buttons in sync when the range changes
/// under them (grid `0–4` vs list `0–2`); key-driven changes are picked up on
/// the next popover open via the show handler in
/// [`build_popover`](crate::ui::toggle_popover::build_popover).
///
/// # Arguments
///
/// * `state` - Application state owning storage and signal handles.
/// * `zoom_out_btn` - Button that decreases the zoom level on click.
/// * `zoom_in_btn` - Button that increases the zoom level on click.
pub fn connect_zoom_handlers(state: &Arc<AppState>, zoom_out_btn: &Button, zoom_in_btn: &Button) {
    let s_zo = Arc::clone(state);
    let out_btn = zoom_out_btn.clone();
    let in_btn = zoom_in_btn.clone();
    state
        .handles
        .lock()
        .retain_signal(zoom_out_btn.connect_clicked(move |_| {
            let mode = s_zo.storage.get_view_mode();
            if apply_zoom_out(&s_zo, mode) {
                notify_zoom_change(&s_zo);
            }
            update_zoom_sensitivity(&s_zo, &out_btn, &in_btn);
        }));

    let s_zin = Arc::clone(state);
    let out_btn2 = zoom_out_btn.clone();
    let in_btn2 = zoom_in_btn.clone();
    state
        .handles
        .lock()
        .retain_signal(zoom_in_btn.connect_clicked(move |_| {
            let mode = s_zin.storage.get_view_mode();
            if apply_zoom_in(&s_zin, mode) {
                notify_zoom_change(&s_zin);
            }
            update_zoom_sensitivity(&s_zin, &out_btn2, &in_btn2);
        }));

    let sub_state = Arc::clone(state);
    let sub_out = zoom_out_btn.clone();
    let sub_in = zoom_in_btn.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let mode_rx = sub_state.view_mode.subscribe();
            while mode_rx.recv().await.is_ok() {
                update_zoom_sensitivity(&sub_state, &sub_out, &sub_in);
            }
        }));
}

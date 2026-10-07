//! View-toggle popover: zoom controls, sort configuration lists, and a
//! preferences entry.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    libadwaita::{
        ButtonContent,
        glib::spawn_future_local,
        gtk::{
            Align::{Center, End},
            Box, Button, Label,
            Orientation::{Horizontal, Vertical},
            Popover, Separator, Widget, Window,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{Albums, Artists},
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::{
        drag::{build_albums_drag_list, build_artists_drag_list},
        preferences::show_preferences_dialog,
        zoom::{GRID_ZOOM_MAX, LIST_ZOOM_MAX},
        zoom_buttons::update_zoom_sensitivity,
    },
};

/// Build the shared preferences entry button.
///
/// # Arguments
///
/// * `state` - Application state for dialog settings.
/// * `parent` - Parent window used to present the preferences dialog.
///
/// # Returns
///
/// * `Button` - Keyboard-focusable flat preferences button.
#[must_use]
pub fn build_preferences_button(state: &Arc<AppState>, parent: &Window) -> Button {
    let prefs_btn = Button::builder()
        .child(
            &ButtonContent::builder()
                .icon_name("preferences-system-symbolic")
                .label("Preferences")
                .build(),
        )
        .tooltip_text("Open preferences")
        .css_classes(["flat"])
        .can_focus(true)
        .hexpand(true)
        .build();
    prefs_btn.update_property(&[PropertyLabel("Preferences")]);
    let state_prefs = Arc::clone(state);
    let parent_prefs = parent.clone();
    state
        .handles
        .lock()
        .retain_signal(prefs_btn.connect_clicked(move |_| {
            show_preferences_dialog(&state_prefs, &parent_prefs);
        }));
    prefs_btn
}

/// Build the popover with zoom controls, sort lists, and a preferences entry.
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings
/// * `parent` - Parent window used to present the preferences dialog
///
/// # Returns
///
/// The popover and the albums/artists sort list widgets for visibility toggling.
pub fn build_popover(state: &Arc<AppState>, parent: &Window) -> (Popover, Widget, Widget) {
    let zoom_box = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    let zoom_controls_box = Box::builder().orientation(Horizontal).spacing(6).build();
    let icon_size_label = Label::builder().label("Icon Size").build();
    zoom_controls_box.append(&icon_size_label);

    let zoom_buttons_box = Box::builder()
        .orientation(Horizontal)
        .css_classes(["linked", "flat"])
        .hexpand(true)
        .halign(End)
        .build();

    let zoom_out_btn = Button::builder()
        .icon_name("zoom-out-symbolic")
        .tooltip_text("Zoom Out")
        .css_classes(["flat"])
        .build();

    let zoom_in_btn = Button::builder()
        .icon_name("zoom-in-symbolic")
        .tooltip_text("Zoom In")
        .css_classes(["flat"])
        .build();

    update_zoom_sensitivity(state, &zoom_out_btn, &zoom_in_btn);

    zoom_buttons_box.append(&zoom_out_btn);
    zoom_buttons_box.append(&zoom_in_btn);
    zoom_controls_box.append(&zoom_buttons_box);
    zoom_box.append(&zoom_controls_box);

    let separator = Separator::new(Horizontal);
    zoom_box.append(&separator);

    let sort_label = Label::builder()
        .label("Sort by")
        .halign(Center)
        .css_classes(["subtitle"])
        .build();
    zoom_box.append(&sort_label);

    let sort_box = Box::builder().build();
    let albums_sort = build_albums_drag_list(state).upcast::<Widget>();
    let artists_sort = build_artists_drag_list(state).upcast::<Widget>();

    let active = state.storage.get_active_tab();
    albums_sort.set_visible(active == Albums);
    artists_sort.set_visible(active == Artists);

    sort_box.append(&albums_sort);
    sort_box.append(&artists_sort);
    zoom_box.append(&sort_box);

    let prefs_separator = Separator::new(Horizontal);
    zoom_box.append(&prefs_separator);

    let prefs_btn = build_preferences_button(state, parent);
    zoom_box.append(&prefs_btn);

    connect_zoom_handlers(state, &zoom_out_btn, &zoom_in_btn);

    let popover = Popover::builder().child(&zoom_box).has_arrow(true).build();
    let sync_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(popover.connect_show(move |_| {
            update_zoom_sensitivity(&sync_state, &zoom_out_btn, &zoom_in_btn);
        }));

    (popover, albums_sort, artists_sort)
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
/// the next popover open via the show handler in [`build_popover`].
fn connect_zoom_handlers(state: &Arc<AppState>, zoom_out_btn: &Button, zoom_in_btn: &Button) {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Context, Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::runtime::AppState,
        storage::view_mode::ViewMode::{Column, Grid},
        ui::{
            toggle_popover::{apply_zoom_in, apply_zoom_out, notify_zoom_change},
            zoom::{GRID_ZOOM_MAX, LIST_ZOOM_MAX},
        },
    };

    #[test]
    fn notify_zoom_change_reaches_both_grids() -> Result<()> {
        let state = Arc::new(AppState::mock().context("failed to build mock app state")?);
        notify_zoom_change(&state);
        ensure!(
            state.albums_zoom_rx.try_recv().is_ok(),
            "a zoom change must reach the album grid"
        );
        ensure!(
            state.artists_zoom_rx.try_recv().is_ok(),
            "a zoom change must reach the artist grid"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_in_clamps_grid_at_max() -> Result<()> {
        let state = Arc::new(AppState::mock().context("failed to build mock app state")?);
        state.storage.set_grid_zoom_level_memory(GRID_ZOOM_MAX - 1);
        _ = apply_zoom_in(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX,
            "zoom in must raise the grid zoom to the maximum"
        );
        _ = apply_zoom_in(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX,
            "zoom in must clamp at the maximum grid zoom"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_out_clamps_grid_at_min() -> Result<()> {
        let state = Arc::new(AppState::mock().context("failed to build mock app state")?);
        state.storage.set_grid_zoom_level_memory(1);
        _ = apply_zoom_out(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == 0,
            "zoom out must lower the grid zoom to the minimum"
        );
        _ = apply_zoom_out(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == 0,
            "zoom out must clamp at the minimum grid zoom"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_in_clamps_list_at_max() -> Result<()> {
        let state = Arc::new(AppState::mock().context("failed to build mock app state")?);
        state.storage.set_list_zoom_level_memory(LIST_ZOOM_MAX - 1);
        _ = apply_zoom_in(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MAX,
            "zoom in must raise the list zoom to the maximum"
        );
        _ = apply_zoom_in(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MAX,
            "zoom in must clamp at the maximum list zoom"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_out_clamps_list_at_min() -> Result<()> {
        let state = Arc::new(AppState::mock().context("failed to build mock app state")?);
        state.storage.set_list_zoom_level_memory(1);
        _ = apply_zoom_out(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == 0,
            "zoom out must lower the list zoom to the minimum"
        );
        _ = apply_zoom_out(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == 0,
            "zoom out must clamp at the minimum list zoom"
        );
        Ok(())
    }
}

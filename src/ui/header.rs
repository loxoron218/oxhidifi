//! `HeaderBar` with Albums/Artists tab buttons and view toggle controls.
//!
//! Uses `AdwViewSwitcher` for tab navigation per GNOME HIG. The switcher
//! is placed in the title widget slot of `AdwHeaderBar`.
//!
//! Provides a `SplitButton` to toggle between grid and column layout views
//! with a popover containing zoom controls and sort configuration.

use std::sync::Arc;

use {
    libadwaita::{
        SplitButton,
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::{Center, End},
            Box, Button, Label,
            Orientation::{Horizontal, Vertical},
            Popover, Separator, Widget, Window,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::AppState,
    storage::settings::{
        ActiveTab::Albums,
        ViewMode::{self, Column, Grid},
    },
    ui::{
        settings::show_preferences_dialog,
        sort_list::{build_albums_sort_list, build_artists_sort_list},
    },
    zoom::{GRID_ZOOM_MAX, GRID_ZOOM_MIN, LIST_ZOOM_MAX, LIST_ZOOM_MIN},
};

/// Persist the view mode setting to storage, logging on failure.
async fn save_view_mode(state: Arc<AppState>, mode: ViewMode) {
    if let Err(err) = state.storage.set_view_mode(mode).await {
        warn!(error = %err, "Failed to set view mode");
    }
}

/// Build the view mode toggle split button with popover.
///
/// Creates a `SplitButton` that switches between grid and column layout
/// on main button click. The arrow dropdown shows a popover with zoom
/// controls (zoom out/in) and sort configuration lists (albums/artists).
///
/// # Arguments
///
/// * `state` - Application state containing storage with settings
///
/// # Returns
///
/// A `SplitButton` with attached popover for view and sort controls.
#[must_use]
pub fn build_view_toggle(state: &Arc<AppState>) -> SplitButton {
    let initial_mode = state.storage.get_view_mode();

    let split_btn = SplitButton::builder()
        .icon_name(initial_mode.icon_name())
        .tooltip_text("Toggle View")
        .can_focus(true)
        .build();
    split_btn.update_property(&[PropertyLabel(initial_mode.tooltip())]);

    let (popover, albums_sort, artists_sort) = build_popover(state);
    split_btn.set_popover(Some(&popover));

    let state_clone = Arc::clone(state);
    split_btn.connect_clicked(move |btn| {
        let current_mode = state_clone.storage.get_view_mode();
        let mode = if current_mode == Grid { Column } else { Grid };
        btn.set_icon_name(mode.icon_name());
        btn.set_tooltip_text(Some(mode.tooltip()));
        let sc = Arc::clone(&state_clone);
        spawn_future_local(save_view_mode(sc, mode));
        if let Err(e) = state_clone.view_mode_tx.send(mode) {
            warn!(error = %e, "Failed to send view mode change");
        }
    });

    subscribe_view_updates(state, &split_btn, &albums_sort, &artists_sort);

    split_btn
}

/// Build the popover with zoom controls and sort lists.
///
/// Returns the popover and the albums/artists sort list widgets for visibility toggling.
fn build_popover(state: &Arc<AppState>) -> (Popover, Widget, Widget) {
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
    let albums_sort = build_albums_sort_list(state).upcast::<Widget>();
    let artists_sort = build_artists_sort_list(state).upcast::<Widget>();

    let show_albums = state.storage.get_active_tab() == Albums;
    albums_sort.set_visible(show_albums);
    artists_sort.set_visible(!show_albums);

    sort_box.append(&albums_sort);
    sort_box.append(&artists_sort);
    zoom_box.append(&sort_box);

    connect_zoom_handlers(state, &zoom_out_btn, &zoom_in_btn);

    (
        Popover::builder().child(&zoom_box).has_arrow(true).build(),
        albums_sort,
        artists_sort,
    )
}

/// Update zoom button sensitivity based on current view mode and zoom level.
fn update_zoom_sensitivity(state: &Arc<AppState>, zoom_out_btn: &Button, zoom_in_btn: &Button) {
    let mode = state.storage.get_view_mode();
    let (min_zoom, max_zoom, current_zoom) = match mode {
        Grid => (
            GRID_ZOOM_MIN,
            GRID_ZOOM_MAX,
            state.storage.get_grid_zoom_level(),
        ),
        Column => (
            LIST_ZOOM_MIN,
            LIST_ZOOM_MAX,
            state.storage.get_list_zoom_level(),
        ),
    };
    zoom_out_btn.set_sensitive(current_zoom > min_zoom);
    zoom_in_btn.set_sensitive(current_zoom < max_zoom);
}

/// Decrease the zoom level for the current view mode.
///
/// Only updates in-memory settings; the debounced disk write happens once
/// per zoom episode in `spawn_listen_sort_zoom` (see `common.rs`), so rapid
/// clicking never spawns a write task per click.
fn apply_zoom_out(state: &AppState, mode: ViewMode) {
    match mode {
        Grid => {
            let level = state.storage.get_grid_zoom_level().saturating_sub(1);
            state.storage.set_grid_zoom_level_memory(level);
        }
        Column => {
            let level = state.storage.get_list_zoom_level().saturating_sub(1);
            state.storage.set_list_zoom_level_memory(level);
        }
    }
}

/// Increase the zoom level for the current view mode.
///
/// Only updates in-memory settings; the debounced disk write happens once
/// per zoom episode in `spawn_listen_sort_zoom` (see `common.rs`).
fn apply_zoom_in(state: &AppState, mode: ViewMode) {
    match mode {
        Grid => {
            let level = state
                .storage
                .get_grid_zoom_level()
                .saturating_add(1)
                .min(GRID_ZOOM_MAX);
            state.storage.set_grid_zoom_level_memory(level);
        }
        Column => {
            let level = state
                .storage
                .get_list_zoom_level()
                .saturating_add(1)
                .min(LIST_ZOOM_MAX);
            state.storage.set_list_zoom_level_memory(level);
        }
    }
}

/// Send a zoom change notification to every library grid.
///
/// `async_channel` receivers compete for messages rather than broadcasting,
/// so each grid subscribes to its own channel and every zoom change must be
/// fanned out to all of them — otherwise the grid not currently active would
/// silently miss (and drop) every other notification.
fn notify_zoom_change(state: &AppState) {
    for tx in [&state.albums_zoom_tx, &state.artists_zoom_tx] {
        if let Err(e) = tx.try_send(()) {
            warn!(error = %e, "Failed to send zoom change");
        }
    }
}

/// Connect zoom button click handlers.
fn connect_zoom_handlers(state: &Arc<AppState>, zoom_out_btn: &Button, zoom_in_btn: &Button) {
    let s_zo = Arc::clone(state);
    let out_btn = zoom_out_btn.clone();
    let in_btn = zoom_in_btn.clone();
    zoom_out_btn.connect_clicked(move |_| {
        let mode = s_zo.storage.get_view_mode();
        apply_zoom_out(&s_zo, mode);
        notify_zoom_change(&s_zo);
        update_zoom_sensitivity(&s_zo, &out_btn, &in_btn);
    });

    let s_zin = Arc::clone(state);
    let out_btn2 = zoom_out_btn.clone();
    let in_btn2 = zoom_in_btn.clone();
    zoom_in_btn.connect_clicked(move |_| {
        let mode = s_zin.storage.get_view_mode();
        apply_zoom_in(&s_zin, mode);
        notify_zoom_change(&s_zin);
        update_zoom_sensitivity(&s_zin, &out_btn2, &in_btn2);
    });
}

/// Subscribe to view mode and active tab changes to update split button icon
/// and toggle sort list visibility.
fn subscribe_view_updates(
    state: &Arc<AppState>,
    split_btn: &SplitButton,
    albums_sort: &Widget,
    artists_sort: &Widget,
) {
    let s = Arc::clone(state);
    let btn = split_btn.clone();
    spawn_future_local(async move {
        let mut vm_rx = s.view_mode_tx.subscribe();
        while vm_rx.changed().await.is_ok() {
            let mode = *vm_rx.borrow();
            btn.set_icon_name(mode.icon_name());
            btn.set_tooltip_text(Some(mode.tooltip()));
        }
    });

    let s2 = Arc::clone(state);
    let albums_sort_btn = albums_sort.clone();
    let artists_sort_btn = artists_sort.clone();
    spawn_future_local(async move {
        let mut tab_rx = s2.active_tab_tx.subscribe();
        while tab_rx.changed().await.is_ok() {
            let is_albums = *tab_rx.borrow() == Albums;
            albums_sort_btn.set_visible(is_albums);
            artists_sort_btn.set_visible(!is_albums);
        }
    });
}

/// Build a header bar with view toggle and preferences button.
///
/// Creates a horizontal box containing the view split button and a
/// gear icon button to open the preferences dialog.
#[must_use]
pub fn build_header_controls(state: &Arc<AppState>, parent: &Window) -> Box {
    let controls = Box::builder().orientation(Horizontal).spacing(6).build();

    let toggle = build_view_toggle(state);
    controls.append(&toggle);

    let prefs_btn = Button::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Preferences")
        .css_classes(["flat"])
        .can_focus(true)
        .build();
    prefs_btn.update_property(&[PropertyLabel("Preferences")]);

    let state_prefs = Arc::clone(state);
    let parent_clone = parent.clone();
    prefs_btn.connect_clicked(move |_| {
        show_preferences_dialog(&state_prefs, &parent_clone);
    });

    controls.append(&prefs_btn);

    controls
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::AppState,
        storage::settings::ViewMode::{Column, Grid},
        ui::header::{apply_zoom_in, apply_zoom_out, build_view_toggle, notify_zoom_change},
        zoom::{GRID_ZOOM_MAX, LIST_ZOOM_MAX},
    };

    #[test]
    fn view_mode_icon_names() {
        assert_eq!(Grid.icon_name(), "view-grid-symbolic");
        assert_eq!(Column.icon_name(), "view-list-symbolic");
    }

    #[test]
    fn view_mode_tooltips() {
        assert_eq!(Grid.tooltip(), "Switch to column view");
        assert_eq!(Column.tooltip(), "Switch to grid view");
    }

    #[test]
    fn notify_zoom_change_reaches_both_grids() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
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
    fn build_view_toggle_sets_initial_icon() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let toggle = build_view_toggle(&state);
        ensure!(toggle.icon_name().as_deref() == Some("view-grid-symbolic"));
        Ok(())
    }

    #[test]
    fn apply_zoom_in_clamps_grid_at_max() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        state.storage.set_grid_zoom_level_memory(GRID_ZOOM_MAX - 1);
        apply_zoom_in(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX,
            "zoom in must raise the grid zoom to the maximum"
        );
        apply_zoom_in(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX,
            "zoom in must clamp at the maximum grid zoom"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_out_clamps_grid_at_min() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        state.storage.set_grid_zoom_level_memory(1);
        apply_zoom_out(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == 0,
            "zoom out must lower the grid zoom to the minimum"
        );
        apply_zoom_out(&state, Grid);
        ensure!(
            state.storage.get_grid_zoom_level() == 0,
            "zoom out must clamp at the minimum grid zoom"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_in_clamps_list_at_max() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        state.storage.set_list_zoom_level_memory(LIST_ZOOM_MAX - 1);
        apply_zoom_in(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MAX,
            "zoom in must raise the list zoom to the maximum"
        );
        apply_zoom_in(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MAX,
            "zoom in must clamp at the maximum list zoom"
        );
        Ok(())
    }

    #[test]
    fn apply_zoom_out_clamps_list_at_min() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        state.storage.set_list_zoom_level_memory(1);
        apply_zoom_out(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == 0,
            "zoom out must lower the list zoom to the minimum"
        );
        apply_zoom_out(&state, Column);
        ensure!(
            state.storage.get_list_zoom_level() == 0,
            "zoom out must clamp at the minimum list zoom"
        );
        Ok(())
    }
}

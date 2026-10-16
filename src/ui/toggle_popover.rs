//! View-toggle popover: zoom controls, sort configuration lists, a
//! preferences entry, and a keyboard shortcuts entry.

use std::sync::Arc;

use libadwaita::{
    ButtonContent,
    gtk::{
        Align::{Center, Start},
        Box, Button, Label,
        Orientation::Horizontal,
        Popover, Separator, Widget, Window,
        accessible::Property::Label as PropertyLabel,
    },
    prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    storage::active_tab::ActiveTab::{Albums, Artists},
    ui::{
        drag::{build_albums_drag_list, build_artists_drag_list},
        popover_zoom::{build_zoom_box, connect_zoom_handlers},
        preferences::show_preferences_dialog,
        shortcuts::build_shortcuts_button,
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
                .halign(Start)
                .css_classes(["prefs-content"])
                .tooltip_text("Preferences content")
                .build(),
        )
        .tooltip_text("Open preferences")
        .css_classes(["flat"])
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

/// Build the popover with zoom controls, sort lists, and preferences and
/// shortcuts entries.
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
    let (zoom_box, zoom_out_btn, zoom_in_btn) = build_zoom_box(state);

    let separator = Separator::builder()
        .orientation(Horizontal)
        .css_classes(["separator"])
        .tooltip_text("Separator")
        .build();
    zoom_box.append(&separator);

    let sort_label = Label::builder()
        .label("Sort by")
        .halign(Center)
        .css_classes(["subtitle"])
        .tooltip_text("Sort options")
        .build();
    zoom_box.append(&sort_label);

    let sort_box = Box::builder()
        .css_classes(["sort-box"])
        .tooltip_text("Sort options")
        .build();
    let albums_sort = build_albums_drag_list(state).upcast::<Widget>();
    let artists_sort = build_artists_drag_list(state).upcast::<Widget>();

    let active = state.storage.get_active_tab();
    albums_sort.set_visible(active == Albums);
    artists_sort.set_visible(active == Artists);

    sort_box.append(&albums_sort);
    sort_box.append(&artists_sort);
    zoom_box.append(&sort_box);

    let prefs_separator = Separator::builder()
        .orientation(Horizontal)
        .css_classes(["separator"])
        .tooltip_text("Separator")
        .build();
    zoom_box.append(&prefs_separator);

    let prefs_btn = build_preferences_button(state, parent);
    zoom_box.append(&prefs_btn);

    let shortcuts_btn = build_shortcuts_button(state, parent);
    zoom_box.append(&shortcuts_btn);

    connect_zoom_handlers(state, &zoom_out_btn, &zoom_in_btn);

    let popover = Popover::builder()
        .child(&zoom_box)
        .has_arrow(true)
        .css_classes(["view-popover"])
        .tooltip_text("View options popover")
        .build();
    popover.update_property(&[PropertyLabel("View options popover")]);
    let sync_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(popover.connect_show(move |_| {
            update_zoom_sensitivity(&sync_state, &zoom_out_btn, &zoom_in_btn);
        }));

    (popover, albums_sort, artists_sort)
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
            popover_zoom::{apply_zoom_in, apply_zoom_out, notify_zoom_change},
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

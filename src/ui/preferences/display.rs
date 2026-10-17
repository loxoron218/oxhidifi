//! View preferences page.

use std::sync::Arc;

use {
    libadwaita::{
        ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage, SwitchRow,
        glib::spawn_future_local,
        gtk::{StringList, accessible::Property::Label},
        prelude::{
            AccessibleExtManual, ComboRowExt, PreferencesDialogExt, PreferencesGroupExt,
            PreferencesPageExt,
        },
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{Albums, Artists, Signal},
        view_mode::ViewMode::{self, Column, Grid},
    },
    ui::navigation::request_active_tab,
};

/// Persist album labels visibility setting and trigger a grid rebuild.
async fn save_album_labels_setting(state: Arc<AppState>, enabled: bool) {
    if let Err(e) = state.storage.set_show_album_labels(enabled).await {
        warn!(error = %e, "Failed to save album labels setting");
    }
    state.refresh.publish();
}

/// Persist view mode and notify listeners.
async fn save_view_mode_setting(state: Arc<AppState>, mode: ViewMode) {
    if let Err(e) = state.storage.set_view_mode(mode).await {
        warn!(error = %e, "Failed to save view mode");
    }
    state.view_mode.send(mode);
}

/// Build the View > Display page.
///
/// Rows keep their default widget classes (`activatable`, `combo`) alongside
/// custom ones: dropping them removes hover/press feedback or leaves the
/// selected value with a black background.
///
/// # Arguments
///
/// * `dialog` - Preferences dialog owning the new page.
/// * `state` - Application state for display settings.
///
/// # Returns
///
/// * `PreferencesPage` - The view page for page-cycle ordering.
pub fn build_view_page(dialog: &PreferencesDialog, state: &Arc<AppState>) -> PreferencesPage {
    let page = PreferencesPage::builder()
        .title("View")
        .icon_name("preferences-desktop-display-symbolic")
        .css_classes(["view-page"])
        .tooltip_text("View display settings")
        .build();
    page.update_property(&[Label("View display settings")]);

    let display_group = PreferencesGroup::builder()
        .title("Display")
        .description("Grid card appearance")
        .css_classes(["display-group"])
        .tooltip_text("Grid card appearance settings")
        .build();

    let labels_row = SwitchRow::builder()
        .title("Album Labels")
        .subtitle("Show album title, artist, and format under cover art")
        .active(state.storage.get_show_album_labels())
        .css_classes(["activatable", "album-labels-row"])
        .tooltip_text("Toggle album labels")
        .build();

    let state_labels = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(labels_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            state_labels
                .handles
                .lock()
                .retain_task(spawn_future_local(save_album_labels_setting(
                    Arc::clone(&state_labels),
                    enabled,
                )));
        }));

    display_group.add(&labels_row);

    let view_mode_model = StringList::new(&["Grid", "Column"]);
    let view_mode_row = ComboRow::builder()
        .title("Default View Mode")
        .subtitle("Initial layout for Albums and Artists views")
        .model(&view_mode_model)
        .css_classes(["combo", "view-mode-row"])
        .tooltip_text("Select default view mode")
        .build();
    view_mode_row.set_selected(match state.storage.get_view_mode() {
        Grid => 0,
        Column => 1,
    });
    let state_vm = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(view_mode_row.connect_selected_notify(move |row| {
            let mode = if row.selected() == 0 { Grid } else { Column };
            let s = Arc::clone(&state_vm);
            state_vm
                .handles
                .lock()
                .retain_task(spawn_future_local(save_view_mode_setting(s, mode)));
        }));
    display_group.add(&view_mode_row);

    let tab_model = StringList::new(&["Albums", "Artists", "Signal"]);
    let tab_row = ComboRow::builder()
        .title("Default Active Tab")
        .subtitle("Initial tab shown when the library opens")
        .model(&tab_model)
        .css_classes(["combo", "active-tab-row"])
        .tooltip_text("Select default active tab")
        .build();
    tab_row.set_selected(match state.storage.get_active_tab() {
        Albums => 0,
        Artists => 1,
        Signal => 2,
    });
    let state_tab = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(tab_row.connect_selected_notify(move |row| {
            let tab = if row.selected() == 0 {
                Albums
            } else if row.selected() == 1 {
                Artists
            } else {
                Signal
            };
            request_active_tab(&state_tab, tab);
        }));
    display_group.add(&tab_row);

    page.add(&display_group);

    dialog.add(&page);
    page
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            ComboRow, PreferencesDialog, PreferencesGroup, SwitchRow,
            glib::object::Cast,
            gtk::{self, Widget, test},
            prelude::{PreferencesDialogExt, PreferencesPageExt, WidgetExt},
        },
    };

    use crate::{
        app::preview_state::{find_descendants, isolated_app_state},
        ui::preferences::display::build_view_page,
    };

    fn view_parts() -> Result<Widget> {
        let state = Arc::new(isolated_app_state()?);
        let dialog = PreferencesDialog::new();
        let built = build_view_page(&dialog, &state);
        ensure!(
            built.title() == "View",
            "view builder must return the view page"
        );
        let Some(page) = dialog.visible_page() else {
            bail!("view page must be visible");
        };
        let root: Widget = page.upcast();
        Ok(root)
    }

    #[test]
    fn view_page_stays_in_focus_chain() -> Result<()> {
        let root = view_parts()?;
        ensure!(
            root.can_focus(),
            "view page must stay focusable so preferences rows keep keyboard focus for \
             Escape-to-close"
        );
        let groups = find_descendants::<PreferencesGroup>(&root);
        ensure!(groups.len() == 1, "view page must own the display group");
        for group in &groups {
            ensure!(
                group.can_focus(),
                "display group must stay focusable so preferences rows keep keyboard focus for \
                 Escape-to-close"
            );
        }
        Ok(())
    }

    #[test]
    fn view_rows_keep_type_classes() -> Result<()> {
        let root = view_parts()?;
        let combos = find_descendants::<ComboRow>(&root);
        ensure!(
            combos.len() == 2,
            "view page must own the view mode and active tab combos"
        );
        for combo in &combos {
            ensure!(
                combo.css_classes().contains(&"combo".into()),
                "view page combos must keep the default `combo` class"
            );
        }
        let switches = find_descendants::<SwitchRow>(&root);
        ensure!(
            switches.len() == 1,
            "view page must own the album labels switch row"
        );
        for switch in &switches {
            ensure!(
                switch.css_classes().contains(&"activatable".into()),
                "album labels row must keep the default `activatable` class"
            );
        }
        Ok(())
    }
}

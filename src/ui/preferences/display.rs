//! View preferences page.

use std::sync::Arc;

use {
    libadwaita::{
        ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage, SwitchRow,
        glib::spawn_future_local,
        gtk::StringList,
        prelude::{
            ActionRowExt, ComboRowExt, PreferencesDialogExt, PreferencesGroupExt,
            PreferencesPageExt, PreferencesRowExt,
        },
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{self, Albums, Artists},
        view_mode::ViewMode::{self, Column, Grid},
    },
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

/// Persist active tab and notify listeners.
async fn save_active_tab_setting(state: Arc<AppState>, tab: ActiveTab) {
    if let Err(e) = state.storage.set_active_tab(tab).await {
        warn!(error = %e, "Failed to save active tab");
    }
    state.active_tab.send(tab);
}

/// Build the View > Display page.
pub(super) fn build_view_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
    let page = PreferencesPage::new();
    page.set_title("View");
    page.set_icon_name(Some("preferences-desktop-display-symbolic"));

    let display_group = PreferencesGroup::new();
    display_group.set_title("Display");
    display_group.set_description(Some("Grid card appearance"));

    let labels_row = SwitchRow::new();
    labels_row.set_title("Album Labels");
    labels_row.set_subtitle("Show album title, artist, and format under cover art");
    labels_row.set_active(state.storage.get_show_album_labels());

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

    let tab_model = StringList::new(&["Albums", "Artists"]);
    let tab_row = ComboRow::builder()
        .title("Default Active Tab")
        .subtitle("Initial tab shown when the library opens")
        .model(&tab_model)
        .build();
    tab_row.set_selected(match state.storage.get_active_tab() {
        Albums => 0,
        Artists => 1,
    });
    let state_tab = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(tab_row.connect_selected_notify(move |row| {
            let tab = if row.selected() == 0 { Albums } else { Artists };
            let s = Arc::clone(&state_tab);
            state_tab
                .handles
                .lock()
                .retain_task(spawn_future_local(save_active_tab_setting(s, tab)));
        }));
    display_group.add(&tab_row);

    page.add(&display_group);

    dialog.add(&page);
}

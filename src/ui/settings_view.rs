//! View preferences page.

use std::sync::Arc;

use {
    libadwaita::{
        PreferencesDialog, PreferencesGroup, PreferencesPage, SwitchRow,
        glib::spawn_future_local,
        prelude::{
            ActionRowExt, PreferencesDialogExt, PreferencesGroupExt, PreferencesPageExt,
            PreferencesRowExt,
        },
    },
    tracing::warn,
};

use crate::app::AppState;

/// Persist album labels visibility setting and trigger a grid rebuild.
async fn save_album_labels_setting(state: Arc<AppState>, enabled: bool) {
    if let Err(e) = state.storage.set_show_album_labels(enabled).await {
        warn!(error = %e, "Failed to save album labels setting");
    }
    state.refresh.publish();
}

/// Build the View > Display page.
pub fn build_view_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
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
    labels_row.connect_active_notify(move |row| {
        let enabled = row.is_active();
        spawn_future_local(save_album_labels_setting(
            Arc::clone(&state_labels),
            enabled,
        ));
    });

    display_group.add(&labels_row);
    page.add(&display_group);

    dialog.add(&page);
}

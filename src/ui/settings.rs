//! `PreferencesDialog` orchestration for library, audio, and view pages.

use std::sync::Arc;

use libadwaita::{
    PreferencesDialog,
    gtk::Window,
    prelude::{AdwDialogExt, PreferencesDialogExt},
};

use crate::{
    app::AppState,
    ui::{
        settings_audio::build_audio_page, settings_library::build_library_page,
        settings_view::build_view_page,
    },
};

/// Build and present the preferences dialog.
pub fn show_preferences_dialog(state: &Arc<AppState>, parent: &Window) {
    let dialog = PreferencesDialog::new();
    dialog.set_search_enabled(false);

    build_library_page(&dialog, state, parent);
    build_audio_page(&dialog, state);
    build_view_page(&dialog, state);

    dialog.present(Some(parent));
}

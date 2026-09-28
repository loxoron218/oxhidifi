//! `PreferencesDialog` orchestration for library, audio, and view pages.

pub mod audio;
pub mod directories;
pub mod display;

use std::sync::Arc;

use libadwaita::{
    PreferencesDialog, PreferencesPage,
    gtk::Window,
    prelude::{AdwDialogExt, PreferencesDialogExt},
};

use crate::{
    app::runtime::AppState,
    ui::preferences::{
        audio::build_audio_page, directories::build_library_page, display::build_view_page,
    },
};

/// Assemble the preferences dialog with library, audio, and view pages.
///
/// The dialog lands on the default view (first page) unless the caller
/// selects the audio page via [`PreferencesDialogExt::set_visible_page`].
///
/// # Arguments
///
/// * `state` - Application state for page settings.
/// * `parent` - Parent window used to present the dialog.
///
/// # Returns
///
/// * `(PreferencesDialog, PreferencesPage)` - Dialog plus its audio page.
fn assemble_preferences_dialog(
    state: &Arc<AppState>,
    parent: &Window,
) -> (PreferencesDialog, PreferencesPage) {
    let dialog = PreferencesDialog::new();
    dialog.set_search_enabled(false);

    build_library_page(&dialog, state, parent);
    let audio = build_audio_page(&dialog, state);
    build_view_page(&dialog, state);

    (dialog, audio)
}

/// Build and present the preferences dialog.
pub fn show_preferences_dialog(state: &Arc<AppState>, parent: &Window) {
    let (dialog, _) = assemble_preferences_dialog(state, parent);

    dialog.present(Some(parent));
}

/// Build and present the preferences dialog on the audio page.
///
/// Used by the Signal tab overflow menu so output/device settings open
/// directly on the audio page; the About action keeps the default view via
/// [`show_preferences_dialog`].
pub fn show_audio_preferences_dialog(state: &Arc<AppState>, parent: &Window) {
    let (dialog, audio) = assemble_preferences_dialog(state, parent);
    dialog.set_visible_page(&audio);

    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, Window, test},
            prelude::{PreferencesDialogExt, PreferencesPageExt},
        },
    };

    use crate::{app::runtime::AppState, ui::preferences::assemble_preferences_dialog};

    #[test]
    fn preferences_land_on_audio_or_default_view() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let parent = Window::new();
        let (dialog, audio) = assemble_preferences_dialog(&state, &parent);
        ensure!(
            dialog
                .visible_page()
                .is_some_and(|page| page.title() == "Library"),
            "default view must land on the library page"
        );
        dialog.set_visible_page(&audio);
        ensure!(
            dialog
                .visible_page()
                .is_some_and(|page| page.title() == "Audio"),
            "settings action must land on the audio page"
        );
        Ok(())
    }
}

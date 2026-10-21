//! `PreferencesDialog` orchestration for library, audio, and view pages.

pub mod audio;
pub mod audio_playback;
pub mod directories;
pub mod display;

use std::sync::Arc;

use libadwaita::{
    PreferencesDialog, PreferencesPage,
    gdk::{Key, ModifierType},
    glib::Propagation::{Proceed, Stop},
    gtk::{EventControllerKey, PropagationPhase::Capture, Window},
    prelude::{AdwDialogExt, Cast, EventControllerExt, PreferencesDialogExt, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    ui::{
        preferences::{
            audio::build_audio_page, directories::build_library_page, display::build_view_page,
        },
        signal_bus::UiHandles,
    },
};

/// Resolve the next preferences page index for a `Ctrl+Tab` key press.
///
/// Only acts when `Ctrl` is pressed. Plain `Ctrl+Tab` advances, while
/// `Ctrl+Shift+Tab` or `Ctrl+ISO_Left_Tab` (emitted for shifted tab on some
/// layouts) goes back, matching the main-window tab switcher.
///
/// # Arguments
///
/// * `current` - Index of the visible page, or `None` when unknown.
/// * `total` - Number of pages in the dialog.
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers.
///
/// # Returns
///
/// * `Option<usize>` - Destination index, or `None` for unrelated keys.
fn next_prefs_page_index(
    current: Option<usize>,
    total: usize,
    key: Key,
    modifiers: ModifierType,
) -> Option<usize> {
    if total == 0 {
        return None;
    }
    if !modifiers.intersects(ModifierType::CONTROL_MASK) {
        return None;
    }
    if key == Key::Tab {
        if modifiers.intersects(ModifierType::SHIFT_MASK) {
            Some(prev_prefs_index(current, total))
        } else {
            Some(next_prefs_index(current, total))
        }
    } else if key == Key::ISO_Left_Tab {
        Some(prev_prefs_index(current, total))
    } else {
        None
    }
}

/// Index of the next preferences page, wrapping around.
///
/// # Arguments
///
/// * `current` - Index of the visible page, or `None` when unknown.
/// * `total` - Number of pages in the dialog.
///
/// # Returns
///
/// * `usize` - Following index, wrapping to the first page.
const fn next_prefs_index(current: Option<usize>, total: usize) -> usize {
    let Some(index) = current else {
        return 0;
    };
    if index >= total {
        return 0;
    }
    let stepped = index.saturating_add(1);
    if stepped >= total { 0 } else { stepped }
}

/// Index of the previous preferences page, wrapping around.
///
/// # Arguments
///
/// * `current` - Index of the visible page, or `None` when unknown.
/// * `total` - Number of pages in the dialog.
///
/// # Returns
///
/// * `usize` - Preceding index, wrapping to the last page.
const fn prev_prefs_index(current: Option<usize>, total: usize) -> usize {
    let Some(index) = current else {
        return total.saturating_sub(1);
    };
    if index == 0 || index >= total {
        total.saturating_sub(1)
    } else {
        index.saturating_sub(1)
    }
}

/// Find the index of the visible preferences page.
///
/// # Arguments
///
/// * `dialog` - Dialog whose visible page is inspected.
/// * `pages` - Pages in dialog order.
///
/// # Returns
///
/// * `Option<usize>` - Index of the visible page, or `None` when unknown.
fn current_prefs_page_index(
    dialog: &PreferencesDialog,
    pages: &[PreferencesPage],
) -> Option<usize> {
    let visible = dialog.visible_page()?;
    pages.iter().position(|page| page == &visible)
}

/// Add `Ctrl+Tab` page cycling to the preferences dialog.
///
/// Forward cycles library, audio, view with wrap; backward uses
/// `Ctrl+Shift+Tab` or `Ctrl+ISO_Left_Tab`. Uses `Capture` so inner rows and
/// search entries never consume the keys first. The connection lives with the
/// dialog via `add_controller`; the handler id is retained in a local handle
/// store that drops with the dialog setup, avoiding a per-open leak in the
/// global handle store.
///
/// # Arguments
///
/// * `dialog` - Dialog receiving the controller.
/// * `pages` - Pages in dialog order.
fn add_prefs_page_controller(dialog: &PreferencesDialog, pages: &[PreferencesPage]) {
    let controller = EventControllerKey::new();
    controller.set_propagation_phase(Capture);
    let stored = pages.to_vec();
    let mut handles = UiHandles::default();
    handles.retain_signal(
        controller.connect_key_pressed(move |ctrl, key, _, modifiers| {
            let Some(widget) = ctrl.widget() else {
                return Proceed;
            };
            let Ok(current_dialog) = widget.downcast::<PreferencesDialog>() else {
                return Proceed;
            };
            let current = current_prefs_page_index(&current_dialog, &stored);
            let Some(next) = next_prefs_page_index(current, stored.len(), key, modifiers) else {
                return Proceed;
            };
            let Some(target) = stored.get(next) else {
                return Proceed;
            };
            current_dialog.set_visible_page(target);
            Stop
        }),
    );
    dialog.add_controller(controller);
}

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

    let library = build_library_page(&dialog, state, parent);
    let audio = build_audio_page(&dialog, state);
    let view = build_view_page(&dialog, state);
    add_prefs_page_controller(&dialog, &[library, audio.clone(), view]);

    (dialog, audio)
}

/// Build and present the preferences dialog.
pub fn show_preferences_dialog(state: &Arc<AppState>, parent: &Window) {
    let (dialog, _) = assemble_preferences_dialog(state, parent);

    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            PreferencesDialog, PreferencesPage,
            gdk::{Key, ModifierType},
            gtk::{self, test},
            prelude::{ListModelExt, PreferencesDialogExt, PreferencesPageExt, WidgetExt},
        },
    };

    use crate::{
        app::{preview_state::test_parent_window, runtime::AppState},
        ui::preferences::{
            assemble_preferences_dialog, current_prefs_page_index, next_prefs_page_index,
        },
    };

    fn assembled_dialog() -> Result<(PreferencesDialog, PreferencesPage)> {
        let state = Arc::new(AppState::mock()?);
        let parent = test_parent_window();
        let (dialog, audio) = assemble_preferences_dialog(&state, &parent);
        Ok((dialog, audio))
    }

    #[test]
    fn preferences_land_on_audio_or_default_view() -> Result<()> {
        let (dialog, audio) = assembled_dialog()?;
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

    #[test]
    fn prefs_page_cycle_matches_main_switcher() -> Result<()> {
        let forward = ModifierType::CONTROL_MASK;
        let backward = ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK;
        let cases = [
            (Some(0), 3, Key::Tab, forward, Some(1)),
            (Some(2), 3, Key::Tab, forward, Some(0)),
            (Some(0), 3, Key::Tab, backward, Some(2)),
            (Some(1), 3, Key::Tab, backward, Some(0)),
            (Some(1), 3, Key::ISO_Left_Tab, forward, Some(0)),
            (None, 3, Key::Tab, forward, Some(0)),
            (None, 3, Key::Tab, backward, Some(2)),
            (Some(0), 3, Key::Tab, ModifierType::empty(), None),
            (Some(0), 3, Key::Tab, ModifierType::SHIFT_MASK, None),
            (Some(0), 3, Key::a, forward, None),
            (Some(0), 0, Key::Tab, forward, None),
        ];
        for (current, total, key, modifiers, expected) in cases {
            ensure!(
                next_prefs_page_index(current, total, key, modifiers) == expected,
                "page cycle mismatch for current {current:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn dialog_tracks_visible_page_index() -> Result<()> {
        let (dialog, audio) = assembled_dialog()?;
        let Some(library) = dialog.visible_page() else {
            bail!("library page must be visible");
        };
        let pages = [library, audio];
        ensure!(
            current_prefs_page_index(&dialog, &pages) == Some(0),
            "visible library page must resolve to the first index"
        );
        dialog.set_visible_page(&pages[1]);
        ensure!(
            current_prefs_page_index(&dialog, &pages) == Some(1),
            "visible audio page must resolve to the second index"
        );
        Ok(())
    }

    #[test]
    fn dialog_attaches_page_cycle_controller() -> Result<()> {
        let (dialog, _) = assembled_dialog()?;
        ensure!(
            dialog.observe_controllers().n_items() > 0,
            "preferences dialog must own a key controller for page cycling"
        );
        Ok(())
    }
}

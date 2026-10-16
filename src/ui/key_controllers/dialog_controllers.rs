//! Dialog shortcut controllers for the main window.
//!
//! Groups the `Ctrl+?` shortcuts-dialog and `Ctrl+,` preferences-dialog
//! controllers. Both work from anywhere, including text entries and pushed
//! detail pages, since neither key has a text editing function to preserve.

use std::sync::Arc;

use libadwaita::{
    ApplicationWindow,
    glib::Propagation::{Proceed, Stop},
    gtk::{EventControllerKey, Window},
    prelude::WidgetExt,
};

use crate::{
    app::runtime::AppState,
    ui::{
        key_bindings::preferences_key::is_preferences_key,
        preferences::show_preferences_dialog,
        shortcuts::{is_shortcuts_key, show_shortcuts_dialog},
    },
};

/// Add the `Ctrl+?` shortcuts-dialog controller to the window.
///
/// Works from anywhere, including text entries (`Ctrl+?` has no editing
/// function to preserve) and pushed detail pages, matching the tab-switch
/// precedent.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `parent` - Parent window used to present the shortcuts dialog.
/// * `state` - Application state owning the retained signal handles.
pub fn add_shortcuts_controller(
    window: &ApplicationWindow,
    parent: &Window,
    state: &Arc<AppState>,
) {
    let dialog_parent = parent.clone();
    let shortcuts_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(
            shortcuts_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if is_shortcuts_key(key, modifiers) {
                    show_shortcuts_dialog(&dialog_parent);
                    Stop
                } else {
                    Proceed
                }
            }),
        );
    window.add_controller(shortcuts_controller);
}

/// Add the `Ctrl+,` preferences-dialog controller to the window.
///
/// Works from anywhere, including text entries (`Ctrl+,` has no editing
/// function to preserve) and pushed detail pages, matching the tab-switch
/// and shortcuts-dialog precedents.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `parent` - Parent window used to present the preferences dialog.
/// * `state` - Application state for dialog settings and signal handles.
pub fn add_preferences_controller(
    window: &ApplicationWindow,
    parent: &Window,
    state: &Arc<AppState>,
) {
    let dialog_parent = parent.clone();
    let dialog_state = Arc::clone(state);
    let preferences_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(
            preferences_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if is_preferences_key(key, modifiers) {
                    show_preferences_dialog(&dialog_state, &dialog_parent);
                    Stop
                } else {
                    Proceed
                }
            }),
        );
    window.add_controller(preferences_controller);
}

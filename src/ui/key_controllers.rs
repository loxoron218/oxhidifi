//! Window key controllers wiring shortcuts to
//! [`key_bindings`](crate::ui::key_bindings) handlers.

pub mod dialog_controllers;
pub mod space_controller;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use libadwaita::{
    ApplicationWindow, NavigationView, OverlaySplitView, PreferencesDialog,
    gdk::{Key, ModifierType},
    glib::{
        Propagation::{Proceed, Stop},
        object::Cast,
    },
    gtk::{EventControllerKey, PropagationPhase::Capture, Widget, Window},
    prelude::{EventControllerExt, GtkWindowExt, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    ui::{
        key_bindings::{
            sidebar_keys::{handle_escape_key, handle_sidebar_toggle},
            tab_switch_key::handle_tab_switch_key,
            text_entry::focus_is_text_entry,
            view_toggle_key::handle_view_toggle_key,
            zoom_key::handle_zoom_key,
        },
        key_controllers::{
            dialog_controllers::{add_preferences_controller, add_shortcuts_controller},
            space_controller::add_space_controller,
        },
        navigation::is_detail_visible,
    },
};

/// Add `Ctrl+G` grid/column toggle controller to the window.
///
/// Ignored while a detail page is pushed (detail covers use fixed sizes) and
/// in text entries so typing keeps working. The `Signal` tab no-op lives in
/// [`handle_view_toggle_key`]. On handled keys the debounced settings write
/// is scheduled; the `view_mode` fan-out already happened synchronously in
/// the handler.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `nav_view` - Navigation view checked for pushed detail pages.
/// * `state` - Application state owning the retained signal handles.
fn add_view_toggle_controller(
    window: &ApplicationWindow,
    nav_view: &NavigationView,
    state: &Arc<AppState>,
) {
    let view_state = Arc::clone(state);
    let view_nav = nav_view.clone();
    let view_window = window.clone();
    let view_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(
            view_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if is_detail_visible(&view_nav) {
                    return Proceed;
                }
                if focus_is_text_entry(view_window.focus().as_ref()) {
                    return Proceed;
                }
                if handle_view_toggle_key(&view_state, key, modifiers) {
                    view_state.storage.save_settings();
                    Stop
                } else {
                    Proceed
                }
            }),
        );
    window.add_controller(view_controller);
}

/// Whether focus is inside a preferences dialog.
///
/// Walks up the widget ancestry so inner rows of a `PreferencesDialog` still
/// count. When `true` the main-window tab switcher yields so the dialog can
/// cycle its own library, audio, and view pages instead.
///
/// # Arguments
///
/// * `focused` - Currently focused widget, if any.
///
/// # Returns
///
/// `true` when the focused widget lives inside a preferences dialog.
fn focus_is_in_prefs_dialog(focused: Option<&Widget>) -> bool {
    let mut current = focused.cloned();
    while let Some(widget) = current {
        if widget.dynamic_cast_ref::<PreferencesDialog>().is_some() {
            return true;
        }
        current = widget.parent();
    }
    false
}

/// Add the `Ctrl+Tab` tab-switch controller to the window.
///
/// Works from text entries (`Ctrl+Tab` has no editing function) and while a
/// detail page is pushed. Yields when focus is inside a preferences dialog so
/// the dialog cycles its own pages. On handled keys the debounced settings write is
/// scheduled; the `active_tab` fan-out already happened synchronously in
/// the handler.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `state` - Application state owning the retained signal handles.
fn add_tab_switch_controller(window: &ApplicationWindow, state: &Arc<AppState>) {
    let tab_state = Arc::clone(state);
    let tab_window = window.clone();
    let tab_controller = EventControllerKey::new();
    tab_controller.set_propagation_phase(Capture);
    state
        .handles
        .lock()
        .retain_signal(
            tab_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if focus_is_in_prefs_dialog(tab_window.focus().as_ref()) {
                    return Proceed;
                }
                if handle_tab_switch_key(&tab_state, key, modifiers) {
                    tab_state.storage.save_settings();
                    Stop
                } else {
                    Proceed
                }
            }),
        );
    window.add_controller(tab_controller);
}

/// Add Escape, sidebar toggle, zoom, view toggle, tab switch, play/pause, shortcuts-dialog, and
/// preferences-dialog key controllers to the window.
///
/// `Ctrl+B` toggles the player panel on every page (including pushed detail
/// pages). Text-input focus wins so `Ctrl+B` keeps its native editing
/// behavior there.
///
/// `Ctrl+Tab` cycles library tabs forward and `Ctrl+Shift+Tab` cycles back,
/// wrapping around. It works from text entries (`Ctrl+Tab` has no editing
/// function) and while a detail page is pushed: the stack change behind the
/// detail reports `Back` via tab tracking, landing on the library with the
/// new tab visible. Inside a preferences dialog it yields so the dialog cycles
/// its own library, audio, and view pages instead.
///
/// Zoom via `Ctrl+`/`Ctrl-` is ignored on the `Signal` tab and while a detail
/// page is pushed, since detail covers use fixed sizes and mutating the
/// background grid zoom would surprise the user on `Back`.
///
/// `Ctrl+G` toggles grid/column on Albums/Artists. It is ignored on the
/// `Signal` tab and while a detail page is pushed, matching the hidden
/// view-switch control, and text-input focus wins so typing keeps working.
///
/// Plain `Space` toggles the open panel when it has an active track, else
/// starts the visible album/artist detail page. The controller uses the
/// `Capture` phase so it runs before the gallery `FlowBox`
/// card-activation handler, making `Space` start playback instead of
/// navigating. Text-input focus and a hidden sidebar on the library fall
/// through to the old behavior.
///
/// `Ctrl+?` opens the keyboard shortcuts dialog from anywhere, including
/// text entries and pushed detail pages.
///
/// `Ctrl+,` opens the preferences dialog from anywhere, including text
/// entries and pushed detail pages.
///
/// # Arguments
///
/// * `window` - Window receiving the controllers.
/// * `split_view` - Split view used for Escape-key and play/pause handling.
/// * `nav_view` - Navigation view checked for pushed detail pages.
/// * `state` - Application state owning the retained signal handles.
/// * `sidebar_intent` - Shared last sidebar intent, cleared when Escape hides the panel so
///   collapse-restore does not resurrect it.
/// * `parent` - Parent window used to present the shortcuts dialog.
pub fn add_key_controllers(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    nav_view: &NavigationView,
    state: &Arc<AppState>,
    sidebar_intent: &Arc<AtomicBool>,
    parent: &Window,
) {
    let esc_split = split_view.clone();
    let esc_intent = Arc::clone(sidebar_intent);
    let esc_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(esc_controller.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Escape && handle_escape_key(&esc_split) {
                esc_intent.store(false, Relaxed);
                Stop
            } else {
                Proceed
            }
        }));
    window.add_controller(esc_controller);

    let toggle_split = split_view.clone();
    let toggle_intent = Arc::clone(sidebar_intent);
    let toggle_window = window.clone();
    let toggle_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(
            toggle_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if modifiers != ModifierType::CONTROL_MASK {
                    return Proceed;
                }
                if key != Key::b && key != Key::B {
                    return Proceed;
                }
                if focus_is_text_entry(toggle_window.focus().as_ref()) {
                    return Proceed;
                }
                handle_sidebar_toggle(&toggle_split);
                toggle_intent.store(toggle_split.shows_sidebar(), Relaxed);
                Stop
            }),
        );
    window.add_controller(toggle_controller);

    let zoom_state = Arc::clone(state);
    let zoom_nav = nav_view.clone();
    let zoom_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(
            zoom_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if is_detail_visible(&zoom_nav) {
                    return Proceed;
                }
                if handle_zoom_key(&zoom_state, key, modifiers) {
                    Stop
                } else {
                    Proceed
                }
            }),
        );
    window.add_controller(zoom_controller);

    add_view_toggle_controller(window, nav_view, state);

    add_space_controller(window, split_view, nav_view, state);

    add_tab_switch_controller(window, state);

    add_shortcuts_controller(window, parent, state);

    add_preferences_controller(window, parent, state);
}

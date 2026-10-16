//! Window key controllers wiring shortcuts to
//! [`key_bindings`](crate::ui::key_bindings) handlers.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use {
    libadwaita::{
        ApplicationWindow, NavigationView, OverlaySplitView,
        gdk::{Key, ModifierType},
        glib::{
            Propagation::{self, Proceed, Stop},
            spawn_future_local,
        },
        gtk::{EventControllerKey, PropagationPhase::Capture, Window},
        prelude::{EventControllerExt, GtkWindowExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    playback::transport::PlaybackTransport,
    ui::{
        gallery::play_action::{play_artist, toggle_or_play_album},
        key_bindings::{
            play_pause_key::{
                SpaceAction::{self, ConsumeIdle, Ignore, PlayAlbum, PlayArtist, ToggleSidebar},
                resolve_space_action,
            },
            sidebar_keys::{handle_escape_key, handle_sidebar_toggle},
            tab_switch_key::handle_tab_switch_key,
            text_entry::focus_is_text_entry,
            view_toggle_key::handle_view_toggle_key,
            zoom_key::handle_zoom_key,
        },
        navigation::{is_detail_visible, visible_detail_target},
        shortcuts::{is_shortcuts_key, show_shortcuts_dialog},
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
fn add_shortcuts_controller(window: &ApplicationWindow, parent: &Window, state: &Arc<AppState>) {
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

/// Add the `Ctrl+Tab` tab-switch controller to the window.
///
/// Works from text entries (`Ctrl+Tab` has no editing function) and while a
/// detail page is pushed. On handled keys the debounced settings write is
/// scheduled; the `active_tab` fan-out already happened synchronously in
/// the handler.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `state` - Application state owning the retained signal handles.
fn add_tab_switch_controller(window: &ApplicationWindow, state: &Arc<AppState>) {
    let tab_state = Arc::clone(state);
    let tab_controller = EventControllerKey::new();
    tab_controller.set_propagation_phase(Capture);
    state
        .handles
        .lock()
        .retain_signal(
            tab_controller.connect_key_pressed(move |_, key, _, modifiers| {
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

/// Toggle sidebar playback, logging failures.
///
/// # Arguments
///
/// * `state` - Application state owning the playback engine.
///
/// # Returns
///
/// * `Propagation` - `Stop` so the key never reaches grid navigation.
fn toggle_sidebar_space(state: &AppState) -> Propagation {
    if let Err(e) = state.playback.toggle_pause() {
        error!(error = %e, "Failed to toggle playback via Space key");
    }
    Stop
}

/// Start the visible album detail page.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `album_id` - Album to toggle or play.
///
/// # Returns
///
/// * `Propagation` - `Stop` so the focused Play button does not double-trigger.
fn start_album_space(state: &Arc<AppState>, album_id: i64) -> Propagation {
    let task_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            toggle_or_play_album(&task_state, album_id).await;
        }));
    Stop
}

/// Start the visible artist detail page.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `artist_id` - Artist to play.
///
/// # Returns
///
/// * `Propagation` - `Stop` so the focused Play button does not double-trigger.
fn start_artist_space(state: &Arc<AppState>, artist_id: i64) -> Propagation {
    let task_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            play_artist(&task_state, artist_id).await;
        }));
    Stop
}

/// Dispatch a resolved `Space` action to its playback effect.
///
/// # Arguments
///
/// * `state` - Application state owning playback and signal handles.
/// * `action` - Resolved sidebar-or-detail action.
///
/// # Returns
///
/// * `Propagation` - `Stop` when handled, `Proceed` when ignored.
fn dispatch_space_action(state: &Arc<AppState>, action: SpaceAction) -> Propagation {
    match action {
        ToggleSidebar | ConsumeIdle => toggle_sidebar_space(state),
        PlayAlbum(album_id) => start_album_space(state, album_id),
        PlayArtist(artist_id) => start_artist_space(state, artist_id),
        Ignore => Proceed,
    }
}

/// Add the plain `Space` playback controller to the window.
///
/// An open panel with an active track toggles first so side-panel music
/// keeps playing; otherwise the visible album/artist detail page starts its
/// playback; otherwise an open-but-idle panel is consumed and a hidden panel
/// falls through. Uses the `Capture` phase so it runs before the gallery
/// `FlowBox` card-activation handler. Text-input focus always wins.
///
/// # Arguments
///
/// * `window` - Window receiving the controller.
/// * `split_view` - Split view whose sidebar visibility gates the shortcut.
/// * `nav_view` - Navigation view providing the visible detail target.
/// * `state` - Application state owning the retained signal handles.
fn add_space_controller(
    window: &ApplicationWindow,
    split_view: &OverlaySplitView,
    nav_view: &NavigationView,
    state: &Arc<AppState>,
) {
    let space_state = Arc::clone(state);
    let space_split = split_view.clone();
    let space_nav = nav_view.clone();
    let space_window = window.clone();
    let space_controller = EventControllerKey::new();
    space_controller.set_propagation_phase(Capture);
    state
        .handles
        .lock()
        .retain_signal(
            space_controller.connect_key_pressed(move |_, key, _, modifiers| {
                if key != Key::space {
                    return Proceed;
                }
                if focus_is_text_entry(space_window.focus().as_ref()) {
                    return Proceed;
                }
                let detail = visible_detail_target(&space_nav);
                let action =
                    resolve_space_action(&space_state, &space_split, detail, key, modifiers);
                dispatch_space_action(&space_state, action)
            }),
        );
    window.add_controller(space_controller);
}

/// Add Escape, sidebar toggle, zoom, view toggle, tab switch, play/pause, and shortcuts-dialog key
/// controllers to the window.
///
/// `Ctrl+B` toggles the player panel on every page (including pushed detail
/// pages). Text-input focus wins so `Ctrl+B` keeps its native editing
/// behavior there.
///
/// `Ctrl+Tab` cycles library tabs forward and `Ctrl+Shift+Tab` cycles back,
/// wrapping around. It works from text entries (`Ctrl+Tab` has no editing
/// function) and while a detail page is pushed: the stack change behind the
/// detail reports `Back` via tab tracking, landing on the library with the
/// new tab visible.
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
}

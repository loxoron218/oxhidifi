//! Application-wide keyboard shortcuts for the main window.
//!
//! Escape hides the sidebar when shown; `Ctrl+`/`Ctrl-` (including the numpad)
//! zoom the active view; plain `Space` toggles play/pause while the player
//! panel is shown. Handlers return whether the key was handled so the
//! window's key controllers can stop propagation.

use libadwaita::{
    OverlaySplitView,
    gdk::{Key, ModifierType},
    glib::object::Cast,
    gtk::{Editable, TextView, Widget},
    prelude::{TextViewExt, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    playback::transport::PlaybackTransport,
    ui::toggle_popover::{apply_zoom_in, apply_zoom_out, notify_zoom_change},
};

/// Hide the sidebar when Escape is pressed and the sidebar is shown.
///
/// Returns `true` when the key was handled (sidebar was visible and got
/// hidden), so the caller can stop propagation.
#[must_use]
pub fn handle_escape_key(split_view: &OverlaySplitView) -> bool {
    if split_view.shows_sidebar() {
        split_view.set_show_sidebar(false);
        true
    } else {
        false
    }
}

/// Whether the focused widget (or any of its ancestors) accepts text input.
///
/// Walks up the widget ancestry so inner text children of an `Entry` still
/// count. Any [`Editable`] (`Entry`, `SearchEntry`, `PasswordEntry`,
/// `SpinButton`) counts, as does an editable [`TextView`]. A read-only
/// `TextView` does not count so `Space` can still toggle playback there.
///
/// # Arguments
///
/// * `focused` - Currently focused widget, if any.
///
/// # Returns
///
/// `true` when typing in the focused widget must win over the global
/// `Space` play/pause shortcut.
#[must_use]
pub fn focus_is_text_entry(focused: Option<&Widget>) -> bool {
    let mut current = focused.cloned();
    while let Some(widget) = current {
        if widget.dynamic_cast_ref::<Editable>().is_some() {
            return true;
        }
        if widget
            .dynamic_cast_ref::<TextView>()
            .is_some_and(|view| view.is_editable())
        {
            return true;
        }
        current = widget.parent();
    }
    false
}

/// Toggle play/pause on plain `Space` while the player panel is shown.
///
/// Only plain `Space` (no modifiers) is handled; `Ctrl+Space` and friends
/// fall through so they keep their default behavior. When the sidebar is
/// hidden the key is ignored so the old `Space` behavior (card activation,
/// button activation) is preserved. Text-input focus must be guarded by the
/// caller via [`focus_is_text_entry`].
///
/// A stopped engine with no track is a safe no-op inside
/// [`PlaybackTransport::toggle_pause`]; the key still counts as handled so a
/// panel-open `Space` never falls through to grid navigation.
///
/// # Arguments
///
/// * `state` - Application state owning the playback engine.
/// * `split_view` - Split view whose sidebar visibility gates the shortcut.
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers; must be empty.
///
/// # Returns
///
/// `true` when playback was toggled (or the no-track no-op ran), `false`
/// otherwise (wrong key, modifiers held, or sidebar hidden).
pub fn handle_play_pause_key(
    state: &AppState,
    split_view: &OverlaySplitView,
    key: Key,
    modifiers: ModifierType,
) -> bool {
    if key != Key::space {
        return false;
    }
    if modifiers != ModifierType::empty() {
        return false;
    }
    if !split_view.shows_sidebar() {
        return false;
    }
    if let Err(e) = state.playback.toggle_pause() {
        tracing::error!(error = %e, "Failed to toggle playback via Space key");
    }
    true
}

/// Zoom the active view in or out on `Ctrl+`/`Ctrl-` (including the numpad).
///
/// Only acts when the `Ctrl` modifier is pressed (ignoring unrelated
/// modifiers such as `ShiftLock`). The current view mode — grid or column —
/// determines whether the grid or list zoom level changes, matching the
/// popover zoom buttons. Zooming the active view fans out through
/// [`notify_zoom_change`] so the coalescer resizes (grid) or rebuilds
/// (column) the live view. At the zoom limits no notification is sent, so
/// at-limit key presses never trigger spurious rebuilds or disk writes.
///
/// Detail pages are guarded by the caller (`add_key_controllers` in
/// `window.rs` returns `Proceed` while a `detail` page is pushed), since
/// detail covers use fixed sizes and this handler only mutates the
/// background library zoom.
///
/// # Returns
///
/// `true` when the zoom level actually changed, `false` otherwise (wrong
/// modifiers, unrelated key, or already at the limit).
pub fn handle_zoom_key(state: &AppState, key: Key, modifiers: ModifierType) -> bool {
    if !modifiers.intersects(ModifierType::CONTROL_MASK) {
        return false;
    }
    let mode = state.storage.get_view_mode();
    if key == Key::plus || key == Key::KP_Add {
        if !apply_zoom_in(state, mode) {
            return false;
        }
        notify_zoom_change(state);
        true
    } else if key == Key::minus || key == Key::KP_Subtract {
        if !apply_zoom_out(state, mode) {
            return false;
        }
        notify_zoom_change(state);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            OverlaySplitView,
            gdk::{Key, ModifierType},
            glib::object::Cast,
            gtk::{self, Entry, Label, SearchEntry, TextView, test as gtk_test},
            prelude::TextViewExt,
        },
    };

    use crate::{
        app::{mocks::isolated_app_state, runtime::AppState},
        playback::state::PlaybackStatus::{Paused, Playing, Stopped},
        storage::view_mode::ViewMode::{Column, Grid},
        ui::{
            key_bindings::{
                focus_is_text_entry, handle_escape_key, handle_play_pause_key, handle_zoom_key,
            },
            zoom::{GRID_ZOOM_MAX, GRID_ZOOM_MIN, LIST_ZOOM_MAX, LIST_ZOOM_MIN},
        },
    };

    #[gtk_test]
    fn escape_key_hides_shown_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            handle_escape_key(&split_view),
            "Escape on a shown sidebar must be handled"
        );
        ensure!(
            !split_view.shows_sidebar(),
            "Escape must hide the shown sidebar"
        );
        Ok(())
    }

    #[gtk_test]
    fn escape_key_ignores_hidden_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(false);
        ensure!(
            !handle_escape_key(&split_view),
            "Escape with a hidden sidebar must not be handled"
        );
        ensure!(!split_view.shows_sidebar());
        Ok(())
    }

    fn grid_state(zoom: u8) -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        state.storage.set_view_mode_memory(Grid);
        state.storage.set_grid_zoom_level_memory(zoom);
        Ok(state)
    }

    fn column_state(zoom: u8) -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        state.storage.set_view_mode_memory(Column);
        state.storage.set_list_zoom_level_memory(zoom);
        Ok(state)
    }

    fn zoom_grid_state(zoom: u8, key: Key) -> Result<Arc<AppState>> {
        let state = grid_state(zoom)?;
        ensure!(handle_zoom_key(&state, key, ModifierType::CONTROL_MASK));
        Ok(state)
    }

    fn zoom_column_state(zoom: u8, key: Key) -> Result<Arc<AppState>> {
        let state = column_state(zoom)?;
        ensure!(handle_zoom_key(&state, key, ModifierType::CONTROL_MASK));
        Ok(state)
    }

    #[test]
    fn ctrl_plus_zooms_in_grid() -> Result<()> {
        let state = zoom_grid_state(2, Key::plus)?;
        ensure!(
            state.storage.get_grid_zoom_level() == 3,
            "Ctrl+ must raise the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_minus_zooms_out_grid() -> Result<()> {
        let state = zoom_grid_state(2, Key::minus)?;
        ensure!(
            state.storage.get_grid_zoom_level() == 1,
            "Ctrl- must lower the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_numpad_add_zooms_in_grid() -> Result<()> {
        let state = zoom_grid_state(GRID_ZOOM_MIN, Key::KP_Add)?;
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MIN + 1,
            "Ctrl+KP_Add must raise the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_numpad_subtract_zooms_out_grid() -> Result<()> {
        let state = zoom_grid_state(GRID_ZOOM_MAX, Key::KP_Subtract)?;
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX - 1,
            "Ctrl+KP_Subtract must lower the grid zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_without_control_modifier_ignored() -> Result<()> {
        let state = grid_state(2)?;
        ensure!(!handle_zoom_key(&state, Key::plus, ModifierType::empty()));
        ensure!(
            state.storage.get_grid_zoom_level() == 2,
            "zoom must not change without the Ctrl modifier"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_clamps_grid_at_max() -> Result<()> {
        let state = grid_state(GRID_ZOOM_MAX)?;
        ensure!(
            !handle_zoom_key(&state, Key::plus, ModifierType::CONTROL_MASK),
            "Ctrl+ at the maximum must be unhandled so no rebuild fires"
        );
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MAX,
            "Ctrl+ must clamp at the maximum grid zoom"
        );
        ensure!(
            state.albums_zoom_rx.is_empty(),
            "at-limit key presses must not notify the grids"
        );
        Ok(())
    }

    #[test]
    fn ctrl_minus_clamps_grid_at_min() -> Result<()> {
        let state = grid_state(GRID_ZOOM_MIN)?;
        ensure!(
            !handle_zoom_key(&state, Key::minus, ModifierType::CONTROL_MASK),
            "Ctrl- at the minimum must be unhandled so no rebuild fires"
        );
        ensure!(
            state.storage.get_grid_zoom_level() == GRID_ZOOM_MIN,
            "Ctrl- must clamp at the minimum grid zoom"
        );
        ensure!(
            state.albums_zoom_rx.is_empty(),
            "at-limit key presses must not notify the grids"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_in_column_view_zooms_list() -> Result<()> {
        let state = zoom_column_state(1, Key::plus)?;
        ensure!(
            state.storage.get_list_zoom_level() == 2,
            "Ctrl+ in column view must raise the list zoom level"
        );
        Ok(())
    }

    #[test]
    fn ctrl_minus_in_column_view_clamps_list_at_min() -> Result<()> {
        let state = column_state(LIST_ZOOM_MIN)?;
        ensure!(
            !handle_zoom_key(&state, Key::minus, ModifierType::CONTROL_MASK),
            "Ctrl- at the minimum must be unhandled so no rebuild fires"
        );
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MIN,
            "Ctrl- in column view must clamp at the minimum list zoom"
        );
        Ok(())
    }

    #[test]
    fn unrelated_key_returns_false() -> Result<()> {
        let state = grid_state(2)?;
        ensure!(!handle_zoom_key(&state, Key::a, ModifierType::CONTROL_MASK));
        ensure!(
            state.storage.get_grid_zoom_level() == 2,
            "an unrelated Ctrl key must not zoom"
        );
        Ok(())
    }

    #[test]
    fn ctrl_plus_in_column_view_clamps_list_at_max() -> Result<()> {
        let state = column_state(LIST_ZOOM_MAX)?;
        ensure!(
            !handle_zoom_key(&state, Key::plus, ModifierType::CONTROL_MASK),
            "Ctrl+ at the maximum must be unhandled so no rebuild fires"
        );
        ensure!(
            state.storage.get_list_zoom_level() == LIST_ZOOM_MAX,
            "Ctrl+ in column view must clamp at the maximum list zoom"
        );
        Ok(())
    }

    fn playing_state() -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        {
            let mut guard = state.playback.shared.state.lock();
            guard.status = Playing;
            guard.current_track_id = Some(1);
        }
        Ok(state)
    }

    fn paused_state() -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        {
            let mut guard = state.playback.shared.state.lock();
            guard.status = Paused;
            guard.current_track_id = Some(1);
        }
        Ok(state)
    }

    #[gtk_test]
    fn space_toggles_playing_to_paused() -> Result<()> {
        let state = playing_state()?;
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            handle_play_pause_key(&state, &split_view, Key::space, ModifierType::empty()),
            "plain Space with an open panel must be handled"
        );
        ensure!(
            state.playback.shared.state.lock().status == Paused,
            "Space must pause playing playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_toggles_paused_to_playing() -> Result<()> {
        let state = paused_state()?;
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            handle_play_pause_key(&state, &split_view, Key::space, ModifierType::empty()),
            "plain Space with an open panel must be handled"
        );
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "Space must resume paused playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_with_hidden_sidebar_is_ignored() -> Result<()> {
        let state = playing_state()?;
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(false);
        ensure!(
            !handle_play_pause_key(&state, &split_view, Key::space, ModifierType::empty()),
            "Space with a hidden panel must fall through to the old behavior"
        );
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "hidden-panel Space must not toggle playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_with_modifiers_is_ignored() -> Result<()> {
        let state = playing_state()?;
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            !handle_play_pause_key(
                &state,
                &split_view,
                Key::space,
                ModifierType::CONTROL_MASK
            ),
            "Ctrl+Space must not toggle playback"
        );
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "modified Space must not toggle playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_with_stopped_engine_is_handled_noop() -> Result<()> {
        let state = Arc::new(isolated_app_state()?);
        {
            let mut guard = state.playback.shared.state.lock();
            guard.status = Stopped;
            guard.current_track_id = None;
        }
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            handle_play_pause_key(&state, &split_view, Key::space, ModifierType::empty()),
            "Space with an open panel must be consumed even without a track"
        );
        ensure!(
            state.playback.shared.state.lock().status == Stopped,
            "Space without a track must leave the stopped engine alone"
        );
        Ok(())
    }

    #[gtk_test]
    fn non_space_key_is_ignored() -> Result<()> {
        let state = playing_state()?;
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            !handle_play_pause_key(&state, &split_view, Key::Return, ModifierType::empty()),
            "Return must not toggle playback"
        );
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "non-Space must not toggle playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_none_is_not_text_entry() -> Result<()> {
        ensure!(
            !focus_is_text_entry(None),
            "no focus must not count as text input"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_plain_label_is_not_text_entry() -> Result<()> {
        let label = Label::new(Some("Title"));
        ensure!(
            !focus_is_text_entry(Some(label.upcast_ref())),
            "a label must not count as text input"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_entry_counts_as_text_entry() -> Result<()> {
        let entry = Entry::new();
        ensure!(
            focus_is_text_entry(Some(entry.upcast_ref())),
            "an entry must win over Space play/pause"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_search_entry_counts_as_text_entry() -> Result<()> {
        let entry = SearchEntry::new();
        ensure!(
            focus_is_text_entry(Some(entry.upcast_ref())),
            "a search entry must win over Space play/pause"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_editable_text_view_counts_as_text_entry() -> Result<()> {
        let view = TextView::new();
        view.set_editable(true);
        ensure!(
            focus_is_text_entry(Some(view.upcast_ref())),
            "an editable text view must win over Space play/pause"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_readonly_text_view_is_not_text_entry() -> Result<()> {
        let view = TextView::new();
        view.set_editable(false);
        ensure!(
            !focus_is_text_entry(Some(view.upcast_ref())),
            "a read-only text view must not block Space play/pause"
        );
        Ok(())
    }
}

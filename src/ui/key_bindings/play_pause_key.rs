//! Play/pause shortcut for the main window.
//!
//! Plain `Space` toggles play/pause while the player panel is shown.
//! Handlers return whether the key was handled so the window's key
//! controllers can stop propagation.

use {
    libadwaita::{
        OverlaySplitView,
        gdk::{Key, ModifierType},
    },
    tracing::error,
};

use crate::{app::runtime::AppState, playback::transport::PlaybackTransport};

/// Toggle play/pause on plain `Space` while the player panel is shown.
///
/// Only plain `Space` (no modifiers) is handled; `Ctrl+Space` and friends
/// fall through so they keep their default behavior. When the sidebar is
/// hidden the key is ignored so the old `Space` behavior (card activation,
/// button activation) is preserved. Text-input focus must be guarded by the
/// caller via [`focus_is_text_entry`](crate::ui::key_bindings::text_entry::focus_is_text_entry).
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
        error!(error = %e, "Failed to toggle playback via Space key");
    }
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            OverlaySplitView,
            gdk::{Key, ModifierType},
            gtk::{self, test as gtk_test},
        },
    };

    use crate::{
        app::{mocks::isolated_app_state, runtime::AppState},
        playback::state::PlaybackStatus::{Paused, Playing, Stopped},
        ui::key_bindings::play_pause_key::handle_play_pause_key,
    };

    #[test]
    fn handler_signature_shape() {
        fn assert_shape<F: Fn(&AppState, &OverlaySplitView, Key, ModifierType) -> bool>(_: F) {}
        assert_shape(handle_play_pause_key);
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

    fn shown_sidebar() -> OverlaySplitView {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        split_view
    }

    fn check_space_toggle(state: &AppState, split_view: &OverlaySplitView) -> Result<()> {
        ensure!(
            handle_play_pause_key(state, split_view, Key::space, ModifierType::empty()),
            "plain Space with an open panel must be handled"
        );
        Ok(())
    }

    fn check_ignored_key(
        state: &AppState,
        key: Key,
        modifiers: ModifierType,
        sidebar_shown: bool,
        unhandled_message: &str,
        unchanged_message: &str,
    ) -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(sidebar_shown);
        ensure!(
            !handle_play_pause_key(state, &split_view, key, modifiers),
            "{unhandled_message}"
        );
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "{unchanged_message}"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_toggles_playing_to_paused() -> Result<()> {
        let state = playing_state()?;
        let split_view = shown_sidebar();
        check_space_toggle(&state, &split_view)?;
        ensure!(
            state.playback.shared.state.lock().status == Paused,
            "Space must pause playing playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_toggles_paused_to_playing() -> Result<()> {
        let state = paused_state()?;
        let split_view = shown_sidebar();
        check_space_toggle(&state, &split_view)?;
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "Space must resume paused playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_with_hidden_sidebar_is_ignored() -> Result<()> {
        let state = playing_state()?;
        check_ignored_key(
            &state,
            Key::space,
            ModifierType::empty(),
            false,
            "Space with a hidden panel must fall through to the old behavior",
            "hidden-panel Space must not toggle playback",
        )
    }

    #[gtk_test]
    fn space_with_modifiers_is_ignored() -> Result<()> {
        let state = playing_state()?;
        check_ignored_key(
            &state,
            Key::space,
            ModifierType::CONTROL_MASK,
            true,
            "Ctrl+Space must not toggle playback",
            "modified Space must not toggle playback",
        )
    }

    #[gtk_test]
    fn space_with_stopped_engine_is_handled_noop() -> Result<()> {
        let state = Arc::new(isolated_app_state()?);
        {
            let mut guard = state.playback.shared.state.lock();
            guard.status = Stopped;
            guard.current_track_id = None;
        }
        let split_view = shown_sidebar();
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
        check_ignored_key(
            &state,
            Key::Return,
            ModifierType::empty(),
            true,
            "Return must not toggle playback",
            "non-Space must not toggle playback",
        )
    }
}

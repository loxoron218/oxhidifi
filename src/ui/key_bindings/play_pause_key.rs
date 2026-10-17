//! Play/pause shortcut for the main window.
//!
//! Plain `Space` toggles play/pause while the player panel has an active
//! track, and starts the visible album/artist detail page otherwise.
//! Handlers return whether the key was handled so the window's key
//! controllers can stop propagation.

use {
    libadwaita::{
        OverlaySplitView,
        gdk::{Key, ModifierType},
    },
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    playback::transport::PlaybackTransport,
    ui::nav_tags::DetailTarget::{self, Album, Artist},
};

/// What plain `Space` should do.
///
/// Keeps the sidebar-first precedence explicit so the window controller and
/// unit tests agree: an open panel with an active track always wins, a
/// visible detail page is next, and an open-but-idle panel on the library
/// is consumed to block grid navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceAction {
    /// Toggle the sidebar queue via [`PlaybackTransport::toggle_pause`].
    ToggleSidebar,
    /// Start or toggle the visible album detail page.
    PlayAlbum(i64),
    /// Start the visible artist detail page.
    PlayArtist(i64),
    /// Consume the key with a safe toggle no-op (open-but-idle panel).
    ConsumeIdle,
    /// Let the key fall through to the focused widget.
    Ignore,
}

/// Whether playback has an active track to toggle.
///
/// A `Some` track id covers `Playing`, `Paused`, and a stopped engine with
/// a saved session track (which [`PlaybackTransport::toggle_pause`] resumes).
/// `None` means there is no sidebar music to keep playing.
///
/// # Arguments
///
/// * `state` - Application state owning the playback engine.
///
/// # Returns
///
/// * `bool` - Whether a toggle would affect a real track.
#[must_use]
pub fn playback_is_active(state: &AppState) -> bool {
    state.playback.state().current_track_id.is_some()
}

/// Resolve plain `Space` to its sidebar-or-detail action.
///
/// Only plain `Space` (no modifiers) is handled; `Ctrl+Space` and friends
/// fall through so they keep their default behavior. Text-input focus must
/// be guarded by the caller via
/// [`focus_is_text_entry`](crate::ui::key_bindings::text_entry::focus_is_text_entry).
///
/// Precedence: an open sidebar with an active track toggles first so open
/// panel music keeps playing; otherwise a visible detail target plays its
/// album/artist; otherwise an open-but-idle sidebar is consumed (the toggle
/// inside is a safe no-op) so library `Space` never falls through to grid
/// navigation.
///
/// # Arguments
///
/// * `state` - Application state owning the playback engine.
/// * `split_view` - Split view whose sidebar visibility gates the shortcut.
/// * `detail` - Visible detail target, if any.
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers; must be empty.
///
/// # Returns
///
/// * `SpaceAction` - Action the caller should execute.
#[must_use]
pub fn resolve_space_action(
    state: &AppState,
    split_view: &OverlaySplitView,
    detail: Option<DetailTarget>,
    key: Key,
    modifiers: ModifierType,
) -> SpaceAction {
    if key != Key::space {
        return SpaceAction::Ignore;
    }
    if modifiers != ModifierType::empty() {
        return SpaceAction::Ignore;
    }
    if split_view.shows_sidebar() && playback_is_active(state) {
        return SpaceAction::ToggleSidebar;
    }
    if let Some(target) = detail {
        match target {
            Album(album_id) => return SpaceAction::PlayAlbum(album_id),
            Artist(artist_id) => return SpaceAction::PlayArtist(artist_id),
        }
    }
    if split_view.shows_sidebar() {
        return SpaceAction::ConsumeIdle;
    }
    SpaceAction::Ignore
}

/// Toggle play/pause on plain `Space` while the player panel is shown.
///
/// Library-only wrapper around [`resolve_space_action`] with no visible
/// detail target. When the sidebar is hidden the key is ignored so the old
/// `Space` behavior (card activation, button activation) is preserved.
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
    match resolve_space_action(state, split_view, None, key, modifiers) {
        SpaceAction::ToggleSidebar | SpaceAction::ConsumeIdle => {
            if let Err(e) = state.playback.toggle_pause() {
                error!(error = %e, "Failed to toggle playback via Space key");
            }
            true
        }
        SpaceAction::PlayAlbum(_) | SpaceAction::PlayArtist(_) | SpaceAction::Ignore => false,
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
            gtk::{self, accessible::Property::Label, test as gtk_test},
            prelude::AccessibleExtManual,
        },
    };

    use crate::{
        app::{preview_state::isolated_app_state, runtime::AppState},
        playback::state::PlaybackStatus::{self, Paused, Playing, Stopped},
        ui::{
            key_bindings::play_pause_key::{
                SpaceAction::{self, Ignore, PlayAlbum, PlayArtist, ToggleSidebar},
                handle_play_pause_key, playback_is_active, resolve_space_action,
            },
            nav_tags::DetailTarget::{self, Album, Artist},
        },
    };

    #[test]
    fn handler_signature_shape() {
        fn assert_shape<F: Fn(&AppState, &OverlaySplitView, Key, ModifierType) -> bool>(_: F) {}
        assert_shape(handle_play_pause_key);
    }

    fn active_state(status: PlaybackStatus) -> Result<Arc<AppState>> {
        let state = Arc::new(isolated_app_state()?);
        {
            let mut guard = state.playback.shared.state.lock();
            guard.status = status;
            guard.current_track_id = (status != Stopped).then_some(1);
        }
        Ok(state)
    }

    fn test_sidebar(shown: bool) -> OverlaySplitView {
        let split_view = OverlaySplitView::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test play sidebar")
            .build();
        split_view.update_property(&[Label("Test play sidebar")]);
        split_view.set_show_sidebar(shown);
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
        let split_view = test_sidebar(sidebar_shown);
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

    fn check_resolve(
        state: &AppState,
        split_view: &OverlaySplitView,
        detail: Option<DetailTarget>,
        expected: SpaceAction,
        message: &str,
    ) -> Result<()> {
        ensure!(
            resolve_space_action(state, split_view, detail, Key::space, ModifierType::empty())
                == expected,
            "{message}"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_toggles_playing_to_paused() -> Result<()> {
        let state = active_state(Playing)?;
        let split_view = test_sidebar(true);
        check_space_toggle(&state, &split_view)?;
        ensure!(
            state.playback.shared.state.lock().status == Paused,
            "Space must pause playing playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_toggles_paused_to_playing() -> Result<()> {
        let state = active_state(Paused)?;
        let split_view = test_sidebar(true);
        check_space_toggle(&state, &split_view)?;
        ensure!(
            state.playback.shared.state.lock().status == Playing,
            "Space must resume paused playback"
        );
        Ok(())
    }

    #[gtk_test]
    fn space_with_hidden_sidebar_is_ignored() -> Result<()> {
        let state = active_state(Playing)?;
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
        let state = active_state(Playing)?;
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
        let state = active_state(Stopped)?;
        let split_view = test_sidebar(true);
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
        let state = active_state(Playing)?;
        check_ignored_key(
            &state,
            Key::Return,
            ModifierType::empty(),
            true,
            "Return must not toggle playback",
            "non-Space must not toggle playback",
        )
    }

    #[gtk_test]
    fn playback_is_active_needs_track_id() -> Result<()> {
        let playing = active_state(Playing)?;
        ensure!(
            playback_is_active(&playing),
            "playing with a track must count as active"
        );
        let stopped = active_state(Stopped)?;
        ensure!(
            !playback_is_active(&stopped),
            "stopped without a track must count as idle"
        );
        Ok(())
    }

    #[gtk_test]
    fn sidebar_with_active_track_wins_over_detail() -> Result<()> {
        let state = active_state(Playing)?;
        check_resolve(
            &state,
            &test_sidebar(true),
            Some(Album(7)),
            ToggleSidebar,
            "open panel music must keep playing instead of starting detail",
        )
    }

    #[gtk_test]
    fn hidden_sidebar_with_detail_starts_detail_playback() -> Result<()> {
        let state = active_state(Stopped)?;
        let split_view = test_sidebar(false);
        check_resolve(
            &state,
            &split_view,
            Some(Album(7)),
            PlayAlbum(7),
            "hidden panel album detail must start its album",
        )?;
        check_resolve(
            &state,
            &split_view,
            Some(Artist(3)),
            PlayArtist(3),
            "hidden panel artist detail must start its artist",
        )
    }

    #[gtk_test]
    fn open_idle_sidebar_with_detail_starts_detail_playback() -> Result<()> {
        let state = active_state(Stopped)?;
        check_resolve(
            &state,
            &test_sidebar(true),
            Some(Album(7)),
            PlayAlbum(7),
            "open-but-idle panel must yield to detail playback",
        )
    }

    #[gtk_test]
    fn hidden_sidebar_without_detail_is_ignored() -> Result<()> {
        let state = active_state(Stopped)?;
        check_resolve(
            &state,
            &test_sidebar(false),
            None,
            Ignore,
            "hidden panel library Space must fall through",
        )
    }
}

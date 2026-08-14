//! Album playback actions shared by grid cards and detail pages.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tracing::{error, info, warn};

use crate::{
    app::runtime::AppState,
    playback::{
        OutputError::{DeviceDisconnected, NoDeviceAvailable},
        PlaybackError::{
            DeviceDisconnected as PlaybackDeviceDisconnected,
            NoDeviceAvailable as PlaybackNoDeviceAvailable, Output,
        },
        state::PlaybackStatus::Playing,
        transport::PlaybackTransport,
    },
    storage::Storage,
};

/// Determine the overlay button icon for an album based on playback state.
pub fn album_play_icon(state: &AppState, album_id: i64) -> &'static str {
    let ps = state.playback.state();
    let is_current = ps.current_album_id == album_id;
    if is_current && ps.status == Playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }
}

/// Toggle pause if this album is currently playing, otherwise play it.
pub async fn toggle_or_play_album(state: &Arc<AppState>, album_id: i64) {
    let is_current = state.playback.state().current_album_id == album_id;
    if is_current {
        if let Err(e) = state.playback.toggle_pause() {
            error!(error = %e, "Failed to toggle pause");
        }
    } else {
        play_album(state, album_id).await;
    }
}

/// Play all tracks in an album by queueing them and starting playback.
///
/// Fetches tracks ordered by track number, queues them, and calls
/// `play_queue` on the playback controller.
async fn play_album(state: &Arc<AppState>, album_id: i64) {
    let tracks = match state.storage.get_tracks_by_album(album_id).await {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, album_id, "Failed to fetch album tracks");
            return;
        }
    };

    if tracks.is_empty() {
        info!(album_id, "Album has no tracks");
        return;
    }

    let track_paths: HashMap<i64, PathBuf> = tracks
        .iter()
        .map(|t| (t.id, PathBuf::from(&t.audio.file_path)))
        .collect();
    let track_ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();

    state.playback.set_track_paths(track_paths);

    if let Err(e) = state.playback.play_queue(track_ids) {
        let error_str = e.to_string();
        warn!(error = %error_str, album_id, "Failed to start album playback");
        let msg = match &e {
            PlaybackNoDeviceAvailable
            | PlaybackDeviceDisconnected
            | Output(NoDeviceAvailable | DeviceDisconnected(_)) => {
                "No audio device available. Check your audio output."
            }
            _ => &error_str,
        };
        if let Err(e) = state.toast_tx.send(msg.into()).await {
            warn!(error = %e, "Failed to enqueue toast notification");
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::runtime::AppState, playback::state::PlaybackStatus::Playing,
        ui::gallery::play_action::album_play_icon,
    };

    #[test]
    fn album_play_icon_stopped_shows_start_icon() -> Result<()> {
        let state = AppState::mock()?;
        ensure!(album_play_icon(&state, 1) == "media-playback-start-symbolic");
        Ok(())
    }

    #[test]
    fn album_play_icon_playing_current_shows_pause_icon() -> Result<()> {
        let state = AppState::mock()?;
        state.playback.shared.state.lock().current_album_id = 42;
        state.playback.shared.state.lock().status = Playing;
        ensure!(album_play_icon(&state, 42) == "media-playback-pause-symbolic");
        ensure!(album_play_icon(&state, 1) == "media-playback-start-symbolic");
        Ok(())
    }
}

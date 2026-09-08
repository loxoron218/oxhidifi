//! Album playback actions shared by grid cards and detail pages.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tracing::{error, info, warn};

use crate::{
    app::runtime::AppState,
    playback::{
        OutputError::{DeviceDisconnected, NoDeviceAvailable},
        PlaybackError::{
            self, DeviceDisconnected as PlaybackDeviceDisconnected,
            NoDeviceAvailable as PlaybackNoDeviceAvailable, Output, QueueFull,
        },
        state::PlaybackStatus::Playing,
        transport::PlaybackTransport,
    },
    storage::Storage,
};

/// Target for playback logging and user feedback.
enum PlaybackTarget {
    /// Album playback identified by album id.
    Album(i64),
    /// Artist playback identified by artist id.
    Artist(i64),
}

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
    let mut tracks = match state.storage.get_tracks_by_album(album_id).await {
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

    tracks.sort_by_key(|t| t.number.unwrap_or(0));

    let track_paths: HashMap<i64, PathBuf> = tracks
        .iter()
        .map(|t| (t.id, PathBuf::from(&t.audio.file_path)))
        .collect();
    let track_ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();

    queue_tracks(
        state,
        track_ids,
        track_paths,
        PlaybackTarget::Album(album_id),
    )
    .await;
}

/// Play all tracks for an artist in (album title, track number) order per FR-022.
///
/// Fetches all albums for the artist, sorts them by title, then for each
/// album fetches its tracks sorted by track number and flattens into a
/// single queue ordered by (album title, track number).
pub async fn play_artist(state: &Arc<AppState>, artist_id: i64) {
    let mut albums = match state.storage.get_albums_by_artist(artist_id).await {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, artist_id, "Failed to fetch artist albums");
            return;
        }
    };

    if albums.is_empty() {
        info!(artist_id, "Artist has no albums");
        return;
    }

    albums.sort_by(|a, b| a.title.cmp(&b.title));

    let mut all_tracks = Vec::new();
    let mut track_paths = HashMap::new();
    for album in &albums {
        let mut tracks = match state.storage.get_tracks_by_album(album.id).await {
            Ok(t) => t,
            Err(e) => {
                warn!(error = %e, album_id = album.id, "Failed to fetch album tracks for artist");
                continue;
            }
        };
        tracks.sort_by_key(|t| t.number.unwrap_or(0));
        for track in &tracks {
            drop(track_paths.insert(track.id, PathBuf::from(&track.audio.file_path)));
        }
        all_tracks.extend(tracks);
    }

    if all_tracks.is_empty() {
        info!(artist_id, "Artist has no tracks");
        return;
    }

    let track_ids: Vec<i64> = all_tracks.iter().map(|t| t.id).collect();
    queue_tracks(
        state,
        track_ids,
        track_paths,
        PlaybackTarget::Artist(artist_id),
    )
    .await;
}

/// Queue tracks and start playback, handling device errors uniformly.
async fn queue_tracks(
    state: &Arc<AppState>,
    track_ids: Vec<i64>,
    track_paths: HashMap<i64, PathBuf>,
    target: PlaybackTarget,
) {
    state.playback.set_track_paths(track_paths);
    if let Err(e) = state.playback.play_queue(track_ids) {
        handle_playback_error(state, &e, target).await;
    }
}

/// Map playback errors to user-facing messages and toast notifications.
async fn handle_playback_error(
    state: &Arc<AppState>,
    error: &PlaybackError,
    target: PlaybackTarget,
) {
    let error_str = error.to_string();
    match target {
        PlaybackTarget::Album(album_id) => {
            warn!(error = %error_str, album_id, "Failed to start album playback");
        }
        PlaybackTarget::Artist(artist_id) => {
            warn!(error = %error_str, artist_id, "Failed to start artist playback");
        }
    }
    if let QueueFull { max } = error {
        let queue_msg = format!("Queue is full (max {max} tracks)");
        warn!(error = %error_str, max, "Queue full — cap reached");
        if let Err(e) = state.toast_tx.send(queue_msg).await {
            warn!(error = %e, "Failed to enqueue toast notification");
        }
        return;
    }
    let msg = match error {
        PlaybackNoDeviceAvailable
        | PlaybackDeviceDisconnected
        | Output(NoDeviceAvailable | DeviceDisconnected(_)) => {
            "No audio device available. Check your audio output."
        }
        _ => error_str.as_str(),
    };
    if let Err(e) = state.toast_tx.send(msg.into()).await {
        warn!(error = %e, "Failed to enqueue toast notification");
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

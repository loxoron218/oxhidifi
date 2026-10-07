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
        shuffle::shuffle_slice,
        state::PlaybackStatus::Playing,
        transport::PlaybackTransport,
    },
    storage::Storage,
    ui::gallery::artist_sort::sort_albums_for_playback,
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
        false,
    )
    .await;
}

/// Play all tracks for an artist in the user's preferred album order.
///
/// Fetches all albums for the artist, sorts them per
/// [`sort_albums_for_playback`], then for each album fetches its tracks
/// sorted by track number and flattens into a single queue. The playback
/// queue is always reset, and ordered playback clears the shuffle flag so
/// `Next`/`Previous` follow album order.
pub async fn play_artist(state: &Arc<AppState>, artist_id: i64) {
    let Some((track_ids, track_paths)) = collect_artist_tracks(state, artist_id).await else {
        return;
    };
    queue_tracks(
        state,
        track_ids,
        track_paths,
        PlaybackTarget::Artist(artist_id),
        true,
    )
    .await;
    if let Err(e) = state.playback.set_shuffle_enabled(false) {
        warn!(error = %e, artist_id, "Failed to clear shuffle mode");
    }
}

/// Play all tracks for an artist in shuffled order with shuffle mode on.
///
/// Collects tracks exactly like [`play_artist`], shuffles the queue, resets
/// the playback queue to that order, then enables shuffle mode so
/// `Next`/`Previous` and auto-advance keep following the shuffled order
/// until ordered playback clears the flag.
pub async fn play_artist_shuffled(state: &Arc<AppState>, artist_id: i64) {
    let Some((mut track_ids, track_paths)) = collect_artist_tracks(state, artist_id).await else {
        return;
    };
    shuffle_slice(&mut track_ids);
    queue_tracks(
        state,
        track_ids,
        track_paths,
        PlaybackTarget::Artist(artist_id),
        true,
    )
    .await;
    if let Err(e) = state.playback.set_shuffle_enabled(true) {
        warn!(error = %e, artist_id, "Failed to enable shuffle mode");
    }
}

/// Collect all track IDs and file paths for an artist in play order.
///
/// Fetches the artist's albums sorted per [`sort_albums_for_playback`], then
/// each album's tracks sorted by track number.
///
/// # Arguments
///
/// * `state` - Application state.
/// * `artist_id` - Artist to collect tracks for.
///
/// # Returns
///
/// * `Some((track_ids, track_paths))` - Track IDs in play order with their file paths, or `None`
///   when the artist has no albums or no tracks.
async fn collect_artist_tracks(
    state: &Arc<AppState>,
    artist_id: i64,
) -> Option<(Vec<i64>, HashMap<i64, PathBuf>)> {
    let mut albums = match state.storage.get_albums_by_artist(artist_id).await {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, artist_id, "Failed to fetch artist albums");
            return None;
        }
    };

    if albums.is_empty() {
        info!(artist_id, "Artist has no albums");
        return None;
    }

    sort_albums_for_playback(&mut albums, state.storage.get_artist_play_order());

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
        return None;
    }

    let track_ids: Vec<i64> = all_tracks.iter().map(|t| t.id).collect();
    Some((track_ids, track_paths))
}

/// Queue tracks and start playback, handling device errors uniformly.
///
/// When `fresh` is set, any existing queue is reset so the given order always
/// takes effect from the first track; otherwise a fully contained queue is
/// preserved and only the current index moves.
///
/// # Arguments
///
/// * `state` - Application state.
/// * `track_ids` - Track IDs in play order.
/// * `track_paths` - File paths keyed by track ID.
/// * `target` - Playback target for logging and user feedback.
/// * `fresh` - Whether to reset the playback queue.
async fn queue_tracks(
    state: &Arc<AppState>,
    track_ids: Vec<i64>,
    track_paths: HashMap<i64, PathBuf>,
    target: PlaybackTarget,
    fresh: bool,
) {
    state.playback.set_track_paths(track_paths);
    let result = if fresh {
        state.playback.play_queue_fresh(track_ids)
    } else {
        state.playback.play_queue(track_ids)
    };
    if let Err(e) = result {
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

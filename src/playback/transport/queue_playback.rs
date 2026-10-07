//! Queue-driven playback helpers for the transport.
//!
//! Extracted from `transport.rs` to keep each file under 400 lines.
//! Implements play-by-track, play-list, play-at-index, and next/previous
//! navigation on [`EngineShared`]. The thin [`PlaybackTransport`] impl in
//! the parent module delegates to these helpers.

use std::{path::PathBuf, sync::Arc};

use tracing::{info, warn};

use crate::{
    playback::{
        PlaybackError::{self, QueueEmpty, QueueFull, Storage, TrackNotFound},
        engine::EngineShared,
        state::PlaybackEvent::QueueChanged,
        worker::start_playback,
    },
    storage::StorageError::QueueFull as StorageQueueFull,
};

/// Resolve the registered file path for a track.
///
/// # Arguments
///
/// * `shared` - Engine state holding the track path registry
/// * `track_id` - Track to resolve
///
/// # Returns
///
/// The file path registered for `track_id`.
fn track_path(shared: &Arc<EngineShared>, track_id: i64) -> Result<PathBuf, PlaybackError> {
    shared
        .track_paths
        .lock()
        .get(&track_id)
        .cloned()
        .ok_or(TrackNotFound(track_id))
}

/// Resolve the path for `track_id` and start playback.
///
/// # Arguments
///
/// * `shared` - Engine state holding paths and the decode worker
/// * `track_id` - Track to start
fn start_track(shared: &Arc<EngineShared>, track_id: i64) -> Result<(), PlaybackError> {
    let path = track_path(shared, track_id)?;
    start_playback(shared, track_id, path);
    Ok(())
}

/// Replace the queue contents, mapping storage errors to playback errors.
///
/// # Arguments
///
/// * `shared` - Engine state holding the queue
/// * `queue` - Replacement track IDs in play order
fn replace_queue(shared: &Arc<EngineShared>, queue: &[i64]) -> Result<(), PlaybackError> {
    shared
        .queue
        .set_queue(queue.to_vec())
        .map_err(|error| match error {
            StorageQueueFull { max } => QueueFull { max },
            error => Storage(error),
        })
}

/// Replace the queue with `queue`, emit the change, and start its first track.
///
/// # Arguments
///
/// * `shared` - Engine state holding the queue
/// * `queue` - Replacement track IDs in play order
fn replace_and_start_first(
    shared: &Arc<EngineShared>,
    queue: Vec<i64>,
) -> Result<(), PlaybackError> {
    replace_queue(shared, &queue)?;
    shared.send_event(&QueueChanged { track_ids: queue });
    let first_id = shared.queue.current().ok_or(QueueEmpty)?;
    start_track(shared, first_id)?;
    Ok(())
}

/// Play a specific track by ID.
///
/// # Errors
///
/// Returns [`PlaybackError::TrackNotFound`] if no path is registered for `track_id`.
pub fn play_single(shared: &Arc<EngineShared>, track_id: i64) -> Result<(), PlaybackError> {
    let path = shared
        .track_paths
        .lock()
        .get(&track_id)
        .cloned()
        .ok_or_else(|| {
            warn!(track_id, "Track not found for playback",);
            TrackNotFound(track_id)
        })?;
    info!(track_id, "Play track command",);
    start_playback(shared, track_id, path);
    Ok(())
}

/// Play a list of track IDs starting from `start_index`.
///
/// The full queue is set in the given order, but playback begins at the
/// track at `start_index`, enabling previous-track navigation. A queue that
/// is already fully contained in the current queue preserves the existing
/// queue and only moves the current index.
///
/// # Errors
///
/// Returns [`PlaybackError::QueueEmpty`] if `queue` is empty, `start_index`
/// is out of bounds, or no current track can be resolved. Returns
/// [`PlaybackError::QueueFull`] if the queue exceeds capacity. Returns
/// [`PlaybackError::TrackNotFound`] if a path is missing.
pub fn play_list_at(
    shared: &Arc<EngineShared>,
    queue: Vec<i64>,
    start_index: usize,
) -> Result<(), PlaybackError> {
    if queue.is_empty() {
        warn!(queue_len = queue.len(), "Play at command with empty queue");
        return Err(QueueEmpty);
    }
    if start_index >= queue.len() {
        warn!(
            start_index,
            queue_len = queue.len(),
            "Start index out of bounds"
        );
        return Err(QueueEmpty);
    }
    let queue_len = queue.len();
    info!(queue_len, start_index, "Play at command",);
    let Some(play_id) = queue.get(start_index).copied() else {
        return Err(QueueEmpty);
    };
    let existing = shared.queue.tracks();
    let should_preserve = !existing.is_empty()
        && existing.contains(&play_id)
        && queue.iter().all(|id| existing.contains(id));
    if should_preserve && let Some(pos) = existing.iter().position(|&id| id == play_id) {
        shared.queue.set_current_index(pos);
        start_track(shared, play_id)?;
        return Ok(());
    }
    replace_queue(shared, &queue)?;
    shared.queue.set_current_index(start_index);
    shared.send_event(&QueueChanged { track_ids: queue });
    let play_id = shared.queue.current().ok_or(QueueEmpty)?;
    start_track(shared, play_id)?;
    Ok(())
}

/// Play a list of track IDs as a queue.
///
/// A queue that is already fully contained in the current queue preserves
/// the existing queue and only moves the current index to the first track.
///
/// # Errors
///
/// Returns [`PlaybackError::QueueEmpty`] if `queue` is empty or no current
/// track can be resolved. Returns [`PlaybackError::QueueFull`] if the queue
/// exceeds capacity. Returns [`PlaybackError::TrackNotFound`] if a path is
/// missing.
pub fn play_list(shared: &Arc<EngineShared>, queue: Vec<i64>) -> Result<(), PlaybackError> {
    if queue.is_empty() {
        warn!(
            queue_len = queue.len(),
            "Play queue command with empty queue"
        );
        return Err(QueueEmpty);
    }
    let queue_len = queue.len();
    info!(queue_len, "Play queue command",);
    let existing = shared.queue.tracks();
    let should_preserve = !existing.is_empty() && queue.iter().all(|id| existing.contains(id));
    if should_preserve
        && let Some(&first_id) = queue.first()
        && let Some(pos) = existing.iter().position(|&id| id == first_id)
    {
        shared.queue.set_current_index(pos);
        start_track(shared, first_id)?;
        return Ok(());
    }
    replace_and_start_first(shared, queue)
}

/// Play a list of track IDs as a fresh queue, always resetting playback.
///
/// Unlike [`play_list`], the current queue is unconditionally replaced — even
/// when every requested track is already queued — so callers like artist-wide
/// Play/Shuffle always get exactly the order they built, starting from the
/// first track. A `QueueChanged` event is always emitted.
///
/// # Errors
///
/// Returns [`PlaybackError::QueueEmpty`] if `queue` is empty or no current
/// track can be resolved. Returns [`PlaybackError::QueueFull`] if the queue
/// exceeds capacity. Returns [`PlaybackError::TrackNotFound`] if a path is
/// missing.
pub fn play_list_fresh(shared: &Arc<EngineShared>, queue: Vec<i64>) -> Result<(), PlaybackError> {
    if queue.is_empty() {
        warn!(
            queue_len = queue.len(),
            "Play fresh queue command with empty queue"
        );
        return Err(QueueEmpty);
    }
    let queue_len = queue.len();
    info!(queue_len, "Play fresh queue command",);
    replace_and_start_first(shared, queue)
}

/// Advance to the next track in the queue.
///
/// # Errors
///
/// Returns [`PlaybackError::QueueEmpty`] if there is no next track.
/// Returns [`PlaybackError::TrackNotFound`] if the next track has no path.
pub fn advance_next(shared: &Arc<EngineShared>) -> Result<(), PlaybackError> {
    let next_id = shared.queue.next().ok_or_else(|| {
        info!(
            direction = "next",
            queue_empty = true,
            "Next track failed — queue empty"
        );
        QueueEmpty
    })?;
    start_track(shared, next_id)?;
    Ok(())
}

/// Go to the previous track in the queue.
///
/// # Errors
///
/// Returns [`PlaybackError::QueueEmpty`] if there is no previous track.
/// Returns [`PlaybackError::TrackNotFound`] if the previous track has no path.
pub fn advance_previous(shared: &Arc<EngineShared>) -> Result<(), PlaybackError> {
    let prev_id = shared.queue.previous().ok_or_else(|| {
        info!(
            direction = "previous",
            queue_empty = true,
            "Previous track failed — queue empty"
        );
        QueueEmpty
    })?;
    start_track(shared, prev_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use {
        anyhow::{Result, anyhow, bail, ensure},
        async_channel::unbounded,
    };

    use crate::playback::{
        PlaybackError::{NoDeviceAvailable, Output, QueueEmpty, TrackNotFound},
        engine::EngineShared,
        state::PlaybackEvent::QueueChanged,
        transport::queue_playback::{
            advance_next, advance_previous, play_list, play_list_at, play_list_fresh,
        },
    };

    fn shared_with_paths(track_ids: &[i64]) -> Arc<EngineShared> {
        let shared = Arc::new(EngineShared::default());
        let paths: HashMap<_, _> = track_ids
            .iter()
            .map(|id| (*id, PathBuf::from(format!("/fake/{id}.flac"))))
            .collect();
        *shared.track_paths.lock() = paths;
        shared
    }

    #[test]
    fn play_list_rejects_empty_queue() {
        let shared = Arc::new(EngineShared::default());
        let result = play_list(&shared, vec![]);
        assert!(matches!(result, Err(QueueEmpty)));
    }

    #[test]
    fn play_list_at_rejects_empty_and_oob() {
        let shared = Arc::new(EngineShared::default());
        assert!(matches!(play_list_at(&shared, vec![], 0), Err(QueueEmpty)));
        assert!(matches!(
            play_list_at(&shared, vec![1, 2, 3], 3),
            Err(QueueEmpty)
        ));
    }

    #[test]
    fn play_list_missing_path_returns_not_found() {
        let shared = Arc::new(EngineShared::default());
        assert!(matches!(
            play_list(&shared, vec![42]),
            Err(TrackNotFound(42))
        ));
    }

    #[test]
    fn play_list_fresh_rejects_empty_queue() {
        let shared = Arc::new(EngineShared::default());
        assert!(matches!(play_list_fresh(&shared, vec![]), Err(QueueEmpty)));
    }

    #[test]
    fn play_list_fresh_replaces_contained_queue() -> Result<()> {
        let shared = shared_with_paths(&[1, 2, 3]);
        shared
            .queue
            .set_queue(vec![1, 2, 3])
            .map_err(|e| anyhow!("{e}"))?;
        shared.queue.set_current_index(2);
        let (tx, rx) = unbounded();
        shared.event_subs.lock().push(tx);
        match play_list_fresh(&shared, vec![3, 2, 1]) {
            Ok(()) | Err(NoDeviceAvailable | Output(_)) => {}
            Err(e) => bail!("unexpected error: {e}"),
        }
        ensure!(
            shared.queue.tracks() == vec![3, 2, 1],
            "fresh play must replace the queue order"
        );
        ensure!(
            shared.queue.current() == Some(3),
            "fresh play must start from the first track"
        );
        let event = rx.try_recv().map_err(|e| anyhow!("{e}"))?;
        let QueueChanged { track_ids } = event else {
            bail!("fresh play must emit QueueChanged");
        };
        ensure!(
            track_ids == vec![3, 2, 1],
            "fresh play must emit the new queue order"
        );
        Ok(())
    }

    #[test]
    fn advance_next_empty_queue_returns_queue_empty() -> Result<()> {
        let shared = shared_with_paths(&[1]);
        shared
            .queue
            .set_queue(vec![1])
            .map_err(|e| anyhow!("{e}"))?;
        ensure!(
            matches!(advance_next(&shared), Err(QueueEmpty)),
            "expected QueueEmpty advancing past single track"
        );
        Ok(())
    }

    #[test]
    fn advance_previous_at_start_returns_queue_empty() {
        let shared = Arc::new(EngineShared::default());
        assert!(matches!(advance_previous(&shared), Err(QueueEmpty)));
    }
}

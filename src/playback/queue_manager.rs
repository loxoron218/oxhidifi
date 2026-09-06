//! Playback queue with current, next, and previous track navigation.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::{
    playback::queue_index::{adjust_index_after_move, adjust_index_after_remove},
    storage::StorageError::{self, QueueFull},
};

/// Thread-safe playback queue managing ordered track IDs with navigation.
#[derive(Debug, Clone)]
pub struct PlaybackQueue {
    /// Shared inner state protected by a mutex.
    inner: Arc<Mutex<PlaybackQueueInner>>,
}

impl PlaybackQueue {
    /// Maximum number of entries allowed in a single queue instance (FR-021).
    pub const MAX_CAPACITY: usize = 100_000;

    /// Create a new empty playback queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PlaybackQueueInner {
                tracks: Vec::new(),
                current_index: None,
            })),
        }
    }

    /// Replace the entire queue and start from the beginning.
    ///
    /// # Errors
    ///
    /// Returns [`crate::storage::StorageError::QueueFull`] if `track_ids`
    /// exceeds [`Self::MAX_CAPACITY`].
    pub fn set_queue(&self, track_ids: Vec<i64>) -> Result<(), crate::storage::StorageError> {
        if track_ids.len() > Self::MAX_CAPACITY {
            return Err(QueueFull {
                max: Self::MAX_CAPACITY,
            });
        }
        let mut inner = self.inner.lock();
        inner.tracks = track_ids;
        inner.current_index = if inner.tracks.is_empty() {
            None
        } else {
            Some(0)
        };
        Ok(())
    }

    /// Set the current index to `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn set_current_index(&self, index: usize) {
        let mut inner = self.inner.lock();
        assert!(
            index < inner.tracks.len(),
            "set_current_index out of bounds"
        );
        inner.current_index = Some(index);
    }

    /// Append a track to the end of the queue.
    ///
    /// # Errors
    ///
    /// Returns [`crate::storage::StorageError::QueueFull`] if the queue already
    /// contains [`Self::MAX_CAPACITY`] entries.
    pub fn append(&self, track_id: i64) -> Result<(), StorageError> {
        let mut inner = self.inner.lock();
        if inner.tracks.len() >= Self::MAX_CAPACITY {
            return Err(QueueFull {
                max: Self::MAX_CAPACITY,
            });
        }
        inner.tracks.push(track_id);
        if inner.current_index.is_none() {
            inner.current_index = Some(0);
        }
        drop(inner);
        Ok(())
    }

    /// Try to append a track, returning `false` when the cap is hit.
    ///
    /// Convenience for UI call-sites that only need a boolean.
    #[must_use]
    pub fn try_append(&self, track_id: i64) -> bool {
        self.append(track_id).is_ok()
    }

    /// Remove a track by its position in the queue.
    ///
    /// Adjusts the current index if necessary.
    #[must_use]
    pub fn remove(&self, position: usize) -> Option<i64> {
        let mut inner = self.inner.lock();
        if position >= inner.tracks.len() {
            return None;
        }
        let removed = inner.tracks.remove(position);
        inner.current_index = inner
            .current_index
            .and_then(|idx| adjust_index_after_remove(idx, position, inner.tracks.len()));
        drop(inner);
        Some(removed)
    }

    /// Move a track from one position to another.
    pub fn move_track(&self, from: usize, to: usize) {
        let mut inner = self.inner.lock();
        if from >= inner.tracks.len() || to >= inner.tracks.len() {
            return;
        }
        let track = inner.tracks.remove(from);
        inner.tracks.insert(to, track);
        inner.current_index = inner
            .current_index
            .map(|idx| adjust_index_after_move(idx, from, to));
    }

    /// Get the next track ID without advancing.
    #[must_use]
    pub fn peek_next(&self) -> Option<i64> {
        let inner = self.inner.lock();
        let idx = inner.current_index?;
        let next = idx.checked_add(1)?;
        inner.tracks.get(next).copied()
    }

    /// Advance to the next track, returning its ID.
    ///
    /// Returns `None` if there is no next track.
    #[must_use]
    pub fn next(&self) -> Option<i64> {
        let mut inner = self.inner.lock();
        let next = inner.current_index.and_then(|idx| idx.checked_add(1))?;
        let id = inner.tracks.get(next).copied()?;
        inner.current_index = Some(next);
        drop(inner);
        Some(id)
    }

    /// Move to the previous track, returning its ID.
    ///
    /// Returns `None` if there is no previous track.
    #[must_use]
    pub fn previous(&self) -> Option<i64> {
        let mut inner = self.inner.lock();
        let prev = inner.current_index.and_then(|idx| idx.checked_sub(1))?;
        let id = inner.tracks.get(prev).copied()?;
        inner.current_index = Some(prev);
        drop(inner);
        Some(id)
    }

    /// Get the ID of the currently playing track.
    #[must_use]
    pub fn current(&self) -> Option<i64> {
        let inner = self.inner.lock();
        let idx = inner.current_index?;
        inner.tracks.get(idx).copied()
    }

    /// Get the index of the currently playing track.
    #[must_use]
    pub fn current_index(&self) -> Option<usize> {
        self.inner.lock().current_index
    }

    /// Get the track IDs of upcoming tracks (after the current one).
    pub fn upcoming(&self) -> Vec<i64> {
        let inner = self.inner.lock();
        inner
            .current_index
            .and_then(|idx| idx.checked_add(1))
            .and_then(|next| inner.tracks.get(next..))
            .map_or_else(Vec::new, ToOwned::to_owned)
    }

    /// Get all track IDs in the queue.
    #[must_use]
    pub fn tracks(&self) -> Vec<i64> {
        let inner = self.inner.lock();
        inner.tracks.clone()
    }

    /// Returns `true` if the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.lock();
        inner.tracks.is_empty()
    }

    /// Returns the number of tracks in the queue.
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock();
        inner.tracks.len()
    }

    /// Clear the queue and reset the current index.
    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.tracks.clear();
        inner.current_index = None;
    }
}

impl Default for PlaybackQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal queue state holding tracks and current position.
#[derive(Debug, Clone)]
struct PlaybackQueueInner {
    /// Ordered list of track IDs.
    tracks: Vec<i64>,
    /// Index of the currently playing track (None if empty).
    current_index: Option<usize>,
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, ensure};

    use crate::playback::queue_manager::PlaybackQueue;

    fn three_track_queue() -> Result<PlaybackQueue> {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20, 30]).map_err(|e| anyhow!("{e}"))?;
        Ok(q)
    }

    #[test]
    fn new_queue_is_empty() -> Result<()> {
        let q = PlaybackQueue::new();
        ensure!(q.is_empty(), "q should be empty");
        ensure!(q.is_empty(), "q.len should be 0");
        ensure!(q.current().is_none(), "q.current should be None");
        Ok(())
    }

    #[test]
    fn set_queue_starts_at_first() -> Result<()> {
        let q = three_track_queue()?;
        ensure!(q.current() == Some(10), "current should be Some(10)");
        ensure!(q.len() == 3, "len should be 3");
        Ok(())
    }

    #[test]
    fn next_advances_index() -> Result<()> {
        let q = three_track_queue()?;
        ensure!(q.next() == Some(20), "next should be Some(20)");
        ensure!(q.next() == Some(30), "next should be Some(30)");
        ensure!(q.next().is_none(), "next should be None at end");
        Ok(())
    }

    #[test]
    fn previous_goes_back() -> Result<()> {
        let q = three_track_queue()?;
        ensure!(q.next() == Some(20), "next should be Some(20)");
        ensure!(q.next() == Some(30), "next should be Some(30)");
        ensure!(q.previous() == Some(20), "previous should be Some(20)");
        ensure!(q.previous() == Some(10), "previous should be Some(10)");
        ensure!(q.previous().is_none(), "previous should be None at start");
        Ok(())
    }

    #[test]
    fn append_adds_to_end() -> Result<()> {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20]).map_err(|e| anyhow!("{e}"))?;
        ensure!(matches!(q.append(30), Ok(())), "append should succeed");
        ensure!(q.len() == 3, "len should be 3");
        ensure!(q.upcoming() == vec![20, 30], "upcoming should be [20,30]");
        Ok(())
    }

    #[test]
    fn remove_adjusts_current() -> Result<()> {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20, 30]).map_err(|e| anyhow!("{e}"))?;

        let removed = q.remove(0);
        ensure!(removed == Some(10), "removed should be Some(10)");

        ensure!(
            q.current() == Some(20),
            "current should be Some(20) after remove"
        );
        Ok(())
    }

    #[test]
    fn clear_resets_everything() -> Result<()> {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20, 30]).map_err(|e| anyhow!("{e}"))?;
        q.clear();
        ensure!(q.is_empty(), "assert failed");
        ensure!(q.current().is_none(), "assert failed");
        Ok(())
    }

    #[test]
    fn set_current_index_updates_current() -> Result<()> {
        let q = three_track_queue()?;
        q.set_current_index(1);
        ensure!(q.current() == Some(20), "current should be Some(20)");
        ensure!(q.current_index() == Some(1), "index should be Some(1)");
        Ok(())
    }

    #[test]
    fn peek_next_returns_upcoming_without_advancing() -> Result<()> {
        let q = three_track_queue()?;
        ensure!(q.peek_next() == Some(20), "peek_next should be Some(20)");
        ensure!(
            q.peek_next() == Some(20),
            "peek_next should still be Some(20)"
        );
        ensure!(q.current_index() == Some(0), "index should remain Some(0)");
        Ok(())
    }

    #[test]
    fn peek_next_none_at_end_or_single_track() -> Result<()> {
        let q = three_track_queue()?;
        q.set_current_index(2);
        ensure!(q.peek_next().is_none(), "assert failed");

        let single = PlaybackQueue::new();
        single.set_queue(vec![42]).map_err(|e| anyhow!("{e}"))?;
        ensure!(single.peek_next().is_none(), "assert failed");
        Ok(())
    }

    #[test]
    fn move_track_moving_current_updates_index_to_target() -> Result<()> {
        let q = three_track_queue()?;
        q.move_track(0, 2);
        ensure!(
            q.current() == Some(10),
            "current should be Some(10) after move"
        );
        ensure!(q.current_index() == Some(2), "index should be Some(2)");
        Ok(())
    }

    #[test]
    fn move_track_moving_before_current_decrements_index() -> Result<()> {
        let q = three_track_queue()?;
        q.set_current_index(2);
        q.move_track(0, 2);
        ensure!(q.current() == Some(30), "current should be Some(30)");
        ensure!(q.current_index() == Some(1), "index should be Some(1)");
        Ok(())
    }

    #[test]
    fn move_track_moving_after_current_increments_index() -> Result<()> {
        let q = three_track_queue()?;
        q.move_track(2, 0);
        ensure!(q.current() == Some(10), "current should be Some(10)");
        ensure!(q.current_index() == Some(1), "index should be Some(1)");
        Ok(())
    }

    #[test]
    fn remove_out_of_bounds_returns_none() -> Result<()> {
        let q = three_track_queue()?;
        ensure!(q.remove(3).is_none(), "assert failed");
        ensure!(q.len() == 3, "assert failed");
        Ok(())
    }

    #[test]
    fn remove_at_end_clamps_index() -> Result<()> {
        let q = three_track_queue()?;
        q.set_current_index(2);
        ensure!(q.remove(2) == Some(30), "remove should return Some(30)");
        ensure!(q.current() == Some(20), "current should be Some(20)");
        ensure!(q.current_index() == Some(1), "index should be Some(1)");
        Ok(())
    }
}

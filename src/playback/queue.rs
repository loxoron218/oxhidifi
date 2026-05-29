//! Playback queue with current, next, and previous track navigation.

use std::sync::Arc;

use parking_lot::Mutex;

/// Thread-safe playback queue managing ordered track IDs with navigation.
#[derive(Debug, Clone)]
pub struct PlaybackQueue {
    /// Shared inner state protected by a mutex.
    inner: Arc<Mutex<PlaybackQueueInner>>,
}

impl PlaybackQueue {
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
    pub fn set_queue(&self, track_ids: Vec<i64>) {
        let mut inner = self.inner.lock();
        inner.tracks = track_ids;
        inner.current_index = if inner.tracks.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Append a track to the end of the queue.
    pub fn append(&self, track_id: i64) {
        let mut inner = self.inner.lock();
        inner.tracks.push(track_id);
        if inner.current_index.is_none() {
            inner.current_index = Some(0);
        }
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

    /// Advance to the next track, returning its ID.
    ///
    /// Returns `None` if there is no next track.
    #[must_use]
    pub fn next(&self) -> Option<i64> {
        let mut inner = self.inner.lock();
        let idx = inner.current_index?;
        let next = idx + 1;
        (next < inner.tracks.len()).then(|| {
            inner.current_index = Some(next);
            inner.tracks[next]
        })
    }

    /// Move to the previous track, returning its ID.
    ///
    /// Returns `None` if there is no previous track.
    #[must_use]
    pub fn previous(&self) -> Option<i64> {
        let mut inner = self.inner.lock();
        let idx = inner.current_index?;
        let result = (idx > 0).then(|| {
            let prev = idx - 1;
            inner.current_index = Some(prev);
            inner.tracks[prev]
        });
        drop(inner);
        result
    }

    /// Get the ID of the currently playing track.
    #[must_use]
    pub fn current(&self) -> Option<i64> {
        let inner = self.inner.lock();
        inner.current_index.map(|idx| inner.tracks[idx])
    }

    /// Get the track IDs of upcoming tracks (after the current one).
    #[must_use]
    pub fn upcoming(&self) -> Vec<i64> {
        let inner = self.inner.lock();
        match inner.current_index {
            Some(idx) if idx + 1 < inner.tracks.len() => inner.tracks[idx + 1..].to_vec(),
            _ => Vec::new(),
        }
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

/// Adjust current index after removing a track at `position`.
fn adjust_index_after_remove(idx: usize, position: usize, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else if idx > position {
        Some(idx - 1)
    } else if idx >= len {
        Some(len - 1)
    } else {
        Some(idx)
    }
}

/// Adjust current index after moving a track from `from` to `to`.
fn adjust_index_after_move(idx: usize, from: usize, to: usize) -> usize {
    if idx == from {
        to
    } else if from < idx && to >= idx {
        idx - 1
    } else if from > idx && to <= idx {
        idx + 1
    } else {
        idx
    }
}

#[cfg(test)]
mod tests {
    use crate::playback::queue::PlaybackQueue;

    fn three_track_queue() -> PlaybackQueue {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20, 30]);
        q
    }

    #[test]
    fn new_queue_is_empty() {
        let q = PlaybackQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        assert!(q.current().is_none());
    }

    #[test]
    fn set_queue_starts_at_first() {
        let q = three_track_queue();
        assert_eq!(q.current(), Some(10));
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn next_advances_index() {
        let q = three_track_queue();
        assert_eq!(q.next(), Some(20));
        assert_eq!(q.next(), Some(30));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn previous_goes_back() {
        let q = three_track_queue();
        assert_eq!(q.next(), Some(20));
        assert_eq!(q.next(), Some(30));
        assert_eq!(q.previous(), Some(20));
        assert_eq!(q.previous(), Some(10));
        assert_eq!(q.previous(), None);
    }

    #[test]
    fn append_adds_to_end() {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20]);
        q.append(30);
        assert_eq!(q.len(), 3);
        assert_eq!(q.upcoming(), vec![20, 30]);
    }

    #[test]
    fn remove_adjusts_current() {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20, 30]);

        let removed = q.remove(0);
        assert_eq!(removed, Some(10));

        assert_eq!(q.current(), Some(20));
    }

    #[test]
    fn clear_resets_everything() {
        let q = PlaybackQueue::new();
        q.set_queue(vec![10, 20, 30]);
        q.clear();
        assert!(q.is_empty());
        assert!(q.current().is_none());
    }
}

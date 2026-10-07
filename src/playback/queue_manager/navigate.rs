//! Playback queue navigation: cursor movement and lookahead.
//!
//! Cursor movement ([`PlaybackQueue::next`], [`PlaybackQueue::previous`])
//! and read-only position queries ([`PlaybackQueue::current`],
//! [`PlaybackQueue::peek_next`], [`PlaybackQueue::upcoming`]) live here as a
//! child module so they can share the parent's queue state while keeping
//! each file under the size limit.

use crate::playback::queue_manager::PlaybackQueue;

impl PlaybackQueue {
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
        let next = self
            .inner
            .lock()
            .current_index
            .and_then(|idx| idx.checked_add(1))?;
        step_to(self, next)
    }

    /// Move to the previous track, returning its ID.
    ///
    /// Returns `None` if there is no previous track.
    #[must_use]
    pub fn previous(&self) -> Option<i64> {
        let prev = self
            .inner
            .lock()
            .current_index
            .and_then(|idx| idx.checked_sub(1))?;
        step_to(self, prev)
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
}

/// Move the cursor to `index` and return that track's ID.
///
/// Returns `None` when `index` is out of bounds.
///
/// # Arguments
///
/// * `queue` - Queue whose cursor to move.
/// * `index` - Position to move the cursor to.
fn step_to(queue: &PlaybackQueue, index: usize) -> Option<i64> {
    let mut inner = queue.inner.lock();
    let id = inner.tracks.get(index).copied()?;
    inner.current_index = Some(index);
    drop(inner);
    Some(id)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow, ensure};

    use crate::playback::queue_manager::PlaybackQueue;

    fn populated_queue() -> Result<PlaybackQueue> {
        let queue = PlaybackQueue::new();
        queue
            .set_queue(vec![10, 20, 30])
            .map_err(|e| anyhow!("{e}"))?;
        Ok(queue)
    }

    #[test]
    fn next_advances_index() -> Result<()> {
        let queue = populated_queue()?;
        ensure!(queue.next() == Some(20), "next should be Some(20)");
        ensure!(queue.next() == Some(30), "next should be Some(30)");
        ensure!(queue.next().is_none(), "next should be None at end");
        Ok(())
    }

    #[test]
    fn previous_goes_back() -> Result<()> {
        let queue = populated_queue()?;
        ensure!(queue.next() == Some(20), "next should be Some(20)");
        ensure!(queue.next() == Some(30), "next should be Some(30)");
        ensure!(queue.previous() == Some(20), "previous should be Some(20)");
        ensure!(queue.previous() == Some(10), "previous should be Some(10)");
        ensure!(
            queue.previous().is_none(),
            "previous should be None at start"
        );
        Ok(())
    }

    #[test]
    fn peek_next_returns_upcoming_without_advancing() -> Result<()> {
        let queue = populated_queue()?;
        ensure!(
            queue.peek_next() == Some(20),
            "peek_next should be Some(20)"
        );
        ensure!(
            queue.peek_next() == Some(20),
            "peek_next should still be Some(20)"
        );
        ensure!(
            queue.current_index() == Some(0),
            "index should remain Some(0)"
        );
        Ok(())
    }

    #[test]
    fn peek_next_none_at_end_or_single_track() -> Result<()> {
        let queue = populated_queue()?;
        queue.set_current_index(2);
        ensure!(queue.peek_next().is_none(), "assert failed");

        let single = PlaybackQueue::new();
        single.set_queue(vec![42]).map_err(|e| anyhow!("{e}"))?;
        ensure!(single.peek_next().is_none(), "assert failed");
        Ok(())
    }
}

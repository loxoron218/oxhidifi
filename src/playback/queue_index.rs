//! Queue index adjustment helpers.
//!
//! Extracted from `queue_manager.rs` to keep each file under 400 lines.

/// Adjust current index after removing a track at `position`.
#[must_use]
pub const fn adjust_index_after_remove(idx: usize, position: usize, len: usize) -> Option<usize> {
    if len == 0 {
        None
    } else if idx > position {
        Some(idx.saturating_sub(1))
    } else if idx >= len {
        Some(len.saturating_sub(1))
    } else {
        Some(idx)
    }
}

/// Adjust current index after moving a track from `from` to `to`.
#[must_use]
pub const fn adjust_index_after_move(idx: usize, from: usize, to: usize) -> usize {
    if idx == from {
        to
    } else if from < idx && to >= idx {
        idx.saturating_sub(1)
    } else if from > idx && to <= idx {
        match idx.checked_add(1) {
            Some(next) => next,
            None => idx,
        }
    } else {
        idx
    }
}

#[cfg(test)]
mod tests {
    use crate::playback::queue_index::{adjust_index_after_move, adjust_index_after_remove};

    #[test]
    fn adjust_index_after_remove_all_cases() {
        assert_eq!(adjust_index_after_remove(0, 0, 0), None);
        assert_eq!(adjust_index_after_remove(2, 1, 3), Some(1));
        assert_eq!(adjust_index_after_remove(2, 2, 2), Some(1));
        assert_eq!(adjust_index_after_remove(0, 1, 3), Some(0));
    }

    #[test]
    fn adjust_index_after_move_all_cases() {
        assert_eq!(adjust_index_after_move(1, 1, 3), 3);
        assert_eq!(adjust_index_after_move(3, 1, 3), 2);
        assert_eq!(adjust_index_after_move(0, 2, 0), 1);
        assert_eq!(adjust_index_after_move(0, 1, 2), 0);
    }
}

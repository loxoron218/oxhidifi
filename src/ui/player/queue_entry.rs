//! Playback queue entry data shared by the queue view and row factory.
//!
//! [`QueueItemData`] describes one visible queue row. Defined here so the
//! queue view in [`playlist`](crate::ui::player::playlist) and the row factory
//! in [`row_factory`](crate::ui::player::row_factory) share the row shape
//! without mutual imports.

/// Data for a single queue entry.
#[derive(Clone, Debug)]
pub struct QueueItemData {
    /// Display name for the track.
    pub name: String,
    /// Whether this is the currently playing track.
    pub is_current: bool,
}

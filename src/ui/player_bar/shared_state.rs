//! Shared state for player bar subcomponents.
//!
//! This module provides thread-safe state structures shared across player bar components.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
};

use libadwaita::glib::SourceId;

/// Thread-safe state shared across player bar subcomponents.
#[derive(Clone)]
pub struct PlayerBarState {
    /// Flag indicating if user is currently seeking.
    pub is_seeking: Arc<AtomicBool>,
    /// Current track duration in milliseconds.
    pub track_duration_ms: Arc<AtomicU64>,
    /// Source ID for position update timeout.
    pub position_update_source: Rc<RefCell<Option<SourceId>>>,
    /// Flag indicating if position updates are currently running.
    pub position_updates_running: Rc<RefCell<bool>>,
    /// Pending seek position in milliseconds.
    pub pending_seek_position: Arc<AtomicU64>,
    /// Pending seek sequence number to identify the latest seek request.
    pub pending_seek_sequence: Arc<AtomicU64>,
}

impl PlayerBarState {
    /// Creates a new player bar state instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_seeking: Arc::new(AtomicBool::new(false)),
            track_duration_ms: Arc::new(AtomicU64::new(0)),
            position_update_source: Rc::new(RefCell::new(None)),
            position_updates_running: Rc::new(RefCell::new(false)),
            pending_seek_position: Arc::new(AtomicU64::new(0)),
            pending_seek_sequence: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Default for PlayerBarState {
    fn default() -> Self {
        Self::new()
    }
}

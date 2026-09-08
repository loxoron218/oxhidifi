//! Shared test fixtures for playback engine tests.
//!
//! Provides helpers to construct `EngineShared` instances with common queue
//! configurations. Extracted to eliminate duplication between
//! `advancer.rs` and `pipeline.rs` test suites.

use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};

use crate::playback::engine::EngineShared;

/// Create a default `EngineShared` wrapped in `Arc`.
#[must_use]
pub fn make_shared_engine() -> Arc<EngineShared> {
    Arc::new(EngineShared::default())
}

/// Create an `EngineShared` with a two-track queue `[1, 2]` and a path for track 2.
///
/// Mirrors the previously duplicated setup in `advancer::tests::two_track_shared_engine`
/// and `pipeline::tests::preload_next_upcoming_sends_command`.
///
/// # Errors
///
/// Returns `anyhow::Error` if the queue cannot be set (e.g., exceeds capacity).
pub fn two_track_shared_engine() -> Result<Arc<EngineShared>> {
    let shared = make_shared_engine();
    shared
        .queue
        .set_queue(vec![1, 2])
        .map_err(|e| anyhow!("{e}"))?;
    drop(
        shared
            .track_paths
            .lock()
            .insert(2, PathBuf::from("/music/two.flac")),
    );
    Ok(shared)
}

/// Create an `EngineShared` with an arbitrary queue.
///
/// # Errors
///
/// Returns `anyhow::Error` if the queue cannot be set (e.g., exceeds capacity).
pub fn engine_with_queue(queue: Vec<i64>) -> Result<Arc<EngineShared>> {
    let shared = make_shared_engine();
    shared.queue.set_queue(queue).map_err(|e| anyhow!("{e}"))?;
    Ok(shared)
}

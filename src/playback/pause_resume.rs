//! Pause/resume toggle helper for the playback transport.
//!
//! Extracted from `transport.rs` to keep each file under 400 lines.

use tracing::{error, info};

use crate::playback::{
    engine::{
        DecodeCommand::{Pause, Resume},
        EngineShared,
    },
    state::{
        PlaybackEvent::{Paused, Resumed},
        PlaybackStatus::{Paused as StatusPaused, Playing},
    },
};

/// Toggle between playing and paused states when playback is active.
pub fn toggle_play_pause(shared: &EngineShared) {
    let mut state = shared.state.lock();
    let was_paused = state.status == StatusPaused;
    let tid = state.current_track_id;
    let (event, cmd) = if was_paused {
        state.status = Playing;
        info!(track_id = tid, "Playback resumed");
        (Resumed, Resume)
    } else {
        state.status = StatusPaused;
        info!(track_id = tid, "Playback paused");
        (Paused, Pause)
    };
    drop(state);

    let cmd_tx = shared.decode_tx.lock();
    if let Some(tx) = cmd_tx.as_ref()
        && let Err(e) = tx.try_send(cmd)
    {
        error!(error = %e, "Failed to send pause/resume command to decode thread");
    }
    drop(cmd_tx);

    shared.send_event(&event);
}

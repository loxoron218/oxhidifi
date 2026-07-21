//! Handling track boundaries: gapless transitions, auto-advance, and finalisation.

use std::{path::PathBuf, sync::Arc};

use tracing::info;

use crate::playback::engine::{
    EngineShared,
    PlaybackEvent::{self, Stopped, TrackFinished},
    PlaybackStatus::{Playing, Stopped as StatusStopped},
};

/// Try to advance to the next track in the queue after a track finishes.
///
/// Advances the queue and updates playback state. Returns `Some((track_id, path))`
/// if a next track is available, or `None` if playback should stop.
pub fn try_auto_advance(
    engine_shared: &Arc<EngineShared>,
    event_to_send: &mut Option<PlaybackEvent>,
) -> Option<(i64, PathBuf)> {
    let next_track = match &event_to_send {
        Some(TrackFinished { .. }) => {
            let next_id = engine_shared.queue.next();
            next_id.and_then(|next_id| {
                let path = engine_shared.track_paths.lock().get(&next_id).cloned()?;
                Some((next_id, path))
            })
        }
        _ => None,
    };
    let (next_id, next_path) = next_track?;

    *engine_shared.decode_tx.lock() = None;

    {
        let mut state = engine_shared.state.lock();
        state.current_track_id = Some(next_id);
        state.current_album_id = -1;
        state.current_path = Some(next_path.clone());
        state.status = Playing;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
    }

    info!(next_id, "Auto-advancing to next track",);

    if let Some(tf_event) = event_to_send.take() {
        engine_shared.send_event(&tf_event);
    }

    Some((next_id, next_path))
}

/// Attempt auto-advance or clean up playback state and emit final events.
///
/// Returns `Some((track_id, path))` if the next track should start playing,
/// or `None` if playback has stopped.
pub fn finalize_track(
    engine_shared: &Arc<EngineShared>,
    event_to_send: &mut Option<PlaybackEvent>,
) -> Option<(i64, PathBuf)> {
    if let Some(track_info) = try_auto_advance(engine_shared, event_to_send) {
        return Some(track_info);
    }

    {
        let mut state = engine_shared.state.lock();
        let had_track = state.current_track_id.is_some();
        state.status = StatusStopped;
        state.current_track_id = None;
        state.current_path = None;
        state.elapsed_seconds = 0.0;
        state.duration_seconds = 0.0;
        drop(state);
        if had_track
            && engine_shared.queue.upcoming().is_empty()
            && event_to_send
                .as_ref()
                .is_some_and(|e| matches!(e, TrackFinished { .. }))
        {
            info!("Playback finished — queue empty, entering idle state");
        }
    }
    *engine_shared.decode_tx.lock() = None;
    if let Some(event) = event_to_send.take() {
        engine_shared.send_event(&event);
    }
    engine_shared.send_event(&Stopped);
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::playback::{
        engine::{
            EngineShared,
            PlaybackEvent::{Paused, TrackFinished},
        },
        track_transition::try_auto_advance,
    };

    fn make_shared_engine() -> Arc<EngineShared> {
        Arc::new(EngineShared::default())
    }

    #[test]
    fn try_auto_advance_returns_none_for_non_track_finished() {
        let shared = make_shared_engine();
        let mut event = Some(Paused);
        let result = try_auto_advance(&shared, &mut event);
        assert!(
            result.is_none(),
            "should return None for non-TrackFinished event"
        );
    }

    #[test]
    fn try_auto_advance_returns_none_when_no_upcoming_track() {
        let shared = make_shared_engine();
        shared.queue.set_queue(vec![1]);
        let mut event = Some(TrackFinished { track_id: 1 });
        let result = try_auto_advance(&shared, &mut event);
        assert!(
            result.is_none(),
            "should return None when queue has only one track"
        );
    }

    #[test]
    fn try_auto_advance_returns_none_when_path_not_found() {
        let shared = make_shared_engine();
        shared.queue.set_queue(vec![1, 2]);
        let mut event = Some(TrackFinished { track_id: 1 });
        let result = try_auto_advance(&shared, &mut event);
        assert!(
            result.is_none(),
            "should return None when path not found for upcoming track"
        );
    }
}

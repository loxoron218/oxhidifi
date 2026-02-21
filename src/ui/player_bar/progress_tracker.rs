//! Position update timer for progress tracking.

use std::{
    sync::{Arc, atomic::Ordering::SeqCst},
    time::Duration,
};

use {
    libadwaita::{
        glib::{ControlFlow::Continue, timeout_add_local},
        gtk::{Label, Scale},
        prelude::RangeExt,
    },
    tracing::warn,
};

use crate::{audio::engine::AudioEngine, ui::player_bar::shared_state::PlayerBarState};

/// Starts position update timer with 100ms interval.
///
/// # Arguments
///
/// * `audio_engine` - Audio engine reference
/// * `progress_scale` - Progress scale widget
/// * `current_time_label` - Current time label widget
/// * `state` - Player bar shared state
///
/// # Panics
///
/// Panics if `position_ms` or `duration_ms` exceed `u32::MAX`, which should never occur in practice.
pub fn start_position_updates(
    audio_engine: Arc<AudioEngine>,
    progress_scale: Scale,
    current_time_label: Label,
    state: &PlayerBarState,
) {
    if state.track_duration_ms.load(SeqCst) == 0 || *state.position_updates_running.borrow() {
        return;
    }

    let is_seeking = state.is_seeking.clone();
    let track_duration_ms = state.track_duration_ms.clone();
    let position_update_source = state.position_update_source.clone();
    let position_updates_running = state.position_updates_running.clone();

    let source_id = timeout_add_local(Duration::from_millis(100), move || {
        if !is_seeking.load(SeqCst)
            && let Some(position_ms) = audio_engine.current_position()
        {
            let duration_ms = track_duration_ms.load(SeqCst);

            if duration_ms > 0
                && position_ms < u64::from(u32::MAX)
                && duration_ms < u64::from(u32::MAX)
            {
                let Some(position_u32) = u32::try_from(position_ms).ok() else {
                    warn!("Failed to convert position to u32");
                    return Continue;
                };
                let Some(duration_u32) = u32::try_from(duration_ms).ok() else {
                    warn!("Failed to convert duration to u32");
                    return Continue;
                };
                let progress = f64::from(position_u32) / f64::from(duration_u32);
                let progress_percent = progress * 100.0;

                progress_scale.set_value(progress_percent);
            }

            let seconds = position_ms / 1000;
            let minutes = seconds / 60;
            let remaining = seconds % 60;
            let time_text = format!("{minutes:02}:{remaining:02}");
            current_time_label.set_label(&time_text);
        }
        Continue
    });

    *position_update_source.borrow_mut() = Some(source_id);
    *position_updates_running.borrow_mut() = true;
}

/// Stops position update timer.
///
/// # Arguments
///
/// * `state` - Player bar shared state
pub fn stop_position_updates(state: &PlayerBarState) {
    if !*state.position_updates_running.borrow() {
        return;
    }

    if let Some(source_id) = state.position_update_source.borrow_mut().take() {
        let () = source_id.remove();
    }
    *state.position_updates_running.borrow_mut() = false;
}

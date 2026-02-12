//! Seek handler with debouncing logic.

use std::{
    sync::{Arc, atomic::Ordering::SeqCst},
    time::Duration,
};

use {
    libadwaita::{
        glib::{ControlFlow::Break, MainContext, Propagation::Proceed, timeout_add_local},
        gtk::{Label, Scale},
        prelude::RangeExt,
    },
    num_traits::cast::FromPrimitive,
    tracing::error,
};

use crate::{audio::engine::AudioEngine, ui::player_bar::shared_state::PlayerBarState};

/// Connects seek handler to progress scale with debouncing.
///
/// # Arguments
///
/// * `progress_scale` - Progress scale widget
/// * `current_time_label` - Current time label widget
/// * `audio_engine` - Audio engine reference
/// * `state` - Player bar shared state
pub fn connect_seek_handler(
    progress_scale: &Scale,
    current_time_label: &Label,
    audio_engine: Arc<AudioEngine>,
    state: &PlayerBarState,
) {
    let is_seeking = state.is_seeking.clone();
    let track_duration_ms = state.track_duration_ms.clone();
    let current_time_label_seek = current_time_label.clone();
    let pending_seek_position = state.pending_seek_position.clone();
    let pending_seek_sequence = state.pending_seek_sequence.clone();

    progress_scale.connect_change_value(move |_scale, _scroll_type, value: f64| {
        let is_seeking = is_seeking.clone();
        let audio_engine = audio_engine.clone();
        let track_duration_ms = track_duration_ms.clone();
        let current_time_label = current_time_label_seek.clone();
        let pending_seek_position = pending_seek_position.clone();
        let pending_seek_sequence = pending_seek_sequence.clone();

        is_seeking.store(true, SeqCst);

        let duration_ms = track_duration_ms.load(SeqCst);
        let position_ms = if duration_ms > 0 {
            let percent = value.clamp(0.0, 100.0).round();
            let percent_u64 = u64::from_f64(percent).unwrap_or_default();
            percent_u64.saturating_mul(duration_ms) / 100
        } else {
            0
        };

        let seconds = position_ms / 1000;
        let minutes = seconds / 60;
        let remaining = seconds % 60;
        let time_text = format!("{minutes:02}:{remaining:02}");
        current_time_label.set_label(&time_text);

        pending_seek_position.store(position_ms, SeqCst);

        let current_sequence = pending_seek_sequence.fetch_add(1, SeqCst).wrapping_add(1);

        timeout_add_local(Duration::from_millis(100), move || {
            let latest_sequence = pending_seek_sequence.load(SeqCst);

            if current_sequence >= latest_sequence {
                let position = pending_seek_position.load(SeqCst);
                let audio_engine = audio_engine.clone();
                let is_seeking = is_seeking.clone();

                MainContext::default().spawn_local(async move {
                    if let Err(e) = audio_engine.seek(position).await {
                        error!(position = %position, error = %e, "Failed to seek to position");
                    }

                    is_seeking.store(false, SeqCst);
                });
            }

            Break
        });

        Proceed
    });
}

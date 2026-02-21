//! `AppState` subscription and event handling for player bar.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use {
    libadwaita::{
        glib::MainContext,
        gtk::{Button, Label, Picture, Scale, ToggleButton},
        prelude::WidgetExt,
    },
    tracing::debug,
};

use crate::{
    audio::engine::{
        AudioEngine,
        PlaybackState::{self, Playing, Stopped},
        TrackInfo,
    },
    state::{
        AppState,
        AppStateEvent::{
            CurrentTrackChanged, ExclusiveModeChanged, PlaybackStateChanged, QueueChanged,
        },
        PlaybackQueue,
    },
};

use crate::ui::player_bar::{
    center_section::{update_playback_state, update_queue_buttons},
    hifi_calculations::HifiPopoverWidgets,
    hifi_popover::update_hifi_indicator,
    left_section::{TrackInfoUpdateContext, update_track_info},
    progress_tracker::{start_position_updates, stop_position_updates},
    shared_state::PlayerBarState,
};

/// Context struct for `AppState` subscription closure.
pub struct StateSubscriptionContext {
    /// Label widget displaying the track title.
    pub title_label: Label,
    /// Label widget displaying the album name.
    pub album_label: Label,
    /// Label widget displaying the artist name.
    pub artist_label: Label,
    /// Label widget displaying the compact format info.
    pub format_label: Label,
    /// Picture widget displaying the album artwork.
    pub artwork: Picture,
    /// Label widget displaying the total track duration.
    pub total_duration_label: Label,
    /// Toggle button for play/pause control.
    pub play_button: ToggleButton,
    /// Button for skipping to previous track.
    pub prev_button: Button,
    /// Button for skipping to next track.
    pub next_button: Button,
    /// Progress scale widget.
    pub progress_scale: Scale,
    /// Label widget displaying the current playback position.
    pub current_time_label: Label,
    /// Track duration in milliseconds.
    pub track_duration_ms: Arc<AtomicU64>,
    /// Hi-Fi indicator button.
    pub hifi_button: Button,
    /// Source format label in popover.
    pub popover_source_format: Label,
    /// Source sample rate label in popover.
    pub popover_source_rate: Label,
    /// Source bit depth label in popover.
    pub popover_source_bits: Label,
    /// Processing status label in popover.
    pub popover_processing: Label,
    /// Output device label in popover.
    pub popover_output_device: Label,
    /// Output format label in popover.
    pub popover_output_format: Label,
    /// Bit-perfect badge widget.
    pub bitperfect_badge: Label,
    /// Gapless badge widget.
    pub gapless_badge: Label,
    /// Hi-Res badge widget.
    pub hires_badge: Label,
    /// Exclusive mode indicator button.
    pub exclusive_mode_button: Button,
}

/// Creates Hi-Fi popover widgets struct from context.
fn create_hifi_popover_widgets(context: &StateSubscriptionContext) -> HifiPopoverWidgets<'_> {
    HifiPopoverWidgets {
        source_format: &context.popover_source_format,
        source_rate: &context.popover_source_rate,
        source_bits: &context.popover_source_bits,
        processing: &context.popover_processing,
        output_device: &context.popover_output_device,
        output_format: &context.popover_output_format,
    }
}

/// Updates Hi-Fi indicator display.
fn update_hifi_display(audio_engine: &Arc<AudioEngine>, context: &StateSubscriptionContext) {
    update_hifi_indicator(
        audio_engine,
        &context.hifi_button,
        &create_hifi_popover_widgets(context),
        &context.bitperfect_badge,
        &context.gapless_badge,
        &context.hires_badge,
    );
}

/// Handles `CurrentTrackChanged` event.
fn handle_current_track_changed(
    track_info: Option<&TrackInfo>,
    audio_engine: &Arc<AudioEngine>,
    context: &StateSubscriptionContext,
    start_position_updates: &impl Fn(),
    stop_position_updates: &impl Fn(),
) {
    debug!("PlayerBar: Current track changed");
    let update_context = TrackInfoUpdateContext {
        title_label: &context.title_label,
        album_label: &context.album_label,
        artist_label: &context.artist_label,
        format_label: &context.format_label,
        artwork: &context.artwork,
        total_duration_label: &context.total_duration_label,
        play_button: &context.play_button,
        prev_button: &context.prev_button,
        next_button: &context.next_button,
    };
    update_track_info(
        track_info,
        &update_context,
        &context.progress_scale,
        &context.current_time_label,
        &context.track_duration_ms,
    );

    if context.track_duration_ms.load(SeqCst) > 0 {
        start_position_updates();
    } else {
        stop_position_updates();
    }

    update_hifi_display(audio_engine, context);
}

/// Handles `PlaybackStateChanged` event.
fn handle_playback_state_changed(
    playback_state: &PlaybackState,
    audio_engine: &Arc<AudioEngine>,
    context: &StateSubscriptionContext,
    start_position_updates: &impl Fn(),
    stop_position_updates: &impl Fn(),
) {
    debug!(state = ?playback_state, "PlayerBar: Playback state changed");
    update_playback_state(playback_state, &context.play_button);

    if matches!(playback_state, Playing) {
        start_position_updates();
    } else if matches!(playback_state, Stopped) {
        stop_position_updates();
    }

    update_hifi_display(audio_engine, context);
}

/// Handles `QueueChanged` event.
fn handle_queue_changed(
    queue: &PlaybackQueue,
    audio_engine: &Arc<AudioEngine>,
    context: &StateSubscriptionContext,
) {
    debug!(track_count = %queue.tracks.len(), "PlayerBar: Queue changed");
    update_queue_buttons(&context.prev_button, &context.next_button, queue);

    update_hifi_display(audio_engine, context);
}

/// Handles `ExclusiveModeChanged` event.
fn handle_exclusive_mode_changed(enabled: bool, context: &StateSubscriptionContext) {
    debug!("PlayerBar: Exclusive mode changed to {}", enabled);

    let button = &context.exclusive_mode_button;

    if enabled {
        button.remove_css_class("inactive");
        button.set_tooltip_text(Some("Exclusive mode active (click to disable)"));
    } else {
        button.add_css_class("inactive");
        button.set_tooltip_text(Some("Exclusive mode disabled (click to enable)"));
    }
}

/// Subscribes to `AppState` changes for reactive updates.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `audio_engine` - Audio engine reference
/// * `context` - Context struct with all widget references
/// * `state` - Player bar shared state
pub fn subscribe_to_state_changes(
    app_state: Arc<AppState>,
    audio_engine: Arc<AudioEngine>,
    context: StateSubscriptionContext,
    state: PlayerBarState,
) {
    debug!("PlayerBar: Subscribing to AppState changes");

    // Initialize exclusive mode button with current setting value
    let settings_manager = app_state.get_settings_manager();
    let current_exclusive_mode = settings_manager.read().get_settings().exclusive_mode;
    handle_exclusive_mode_changed(current_exclusive_mode, &context);

    MainContext::default().spawn_local(async move {
        let state_clone = state.clone();

        let start_position_updates = {
            let audio_engine = audio_engine.clone();
            let progress_scale = context.progress_scale.clone();
            let current_time_label = context.current_time_label.clone();
            let state = state_clone.clone();

            move || {
                start_position_updates(
                    audio_engine.clone(),
                    progress_scale.clone(),
                    current_time_label.clone(),
                    &state,
                );
            }
        };

        let stop_position_updates = {
            let state = state_clone.clone();

            move || {
                stop_position_updates(&state);
            }
        };

        let receiver = app_state.subscribe();
        loop {
            if let Ok(event) = receiver.recv().await {
                match event {
                    CurrentTrackChanged(track_info) => {
                        handle_current_track_changed(
                            (*track_info).as_ref(),
                            &audio_engine,
                            &context,
                            &start_position_updates,
                            &stop_position_updates,
                        );
                    }
                    PlaybackStateChanged(playback_state) => {
                        handle_playback_state_changed(
                            &playback_state,
                            &audio_engine,
                            &context,
                            &start_position_updates,
                            &stop_position_updates,
                        );
                    }
                    QueueChanged(queue) => {
                        handle_queue_changed(&queue, &audio_engine, &context);
                    }
                    ExclusiveModeChanged { enabled } => {
                        handle_exclusive_mode_changed(enabled, &context);
                    }
                    _ => {}
                }
            } else {
                debug!("PlayerBar state subscription channel closed");
                break;
            }
        }
    });
}

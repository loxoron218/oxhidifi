//! Playback control widgets: transport buttons, seek slider, and volume control.

use std::sync::{Arc, atomic::Ordering::Release};

use {
    libadwaita::{
        glib::{Propagation::Proceed, spawn_future_local},
        gtk::{
            Align::{Center, End, Start},
            Box, Button, GestureClick, Label,
            Orientation::{Horizontal, Vertical},
            Scale,
            accessible::Property::Label as PropertyLabel,
            prelude::{GestureSingleExt, RangeExt},
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, ScaleExt, WidgetExt},
    },
    tracing::{error, warn},
};

use crate::{
    app::AppState,
    playback::{
        engine::{MuteState::Unmuted, PlaybackController, PlaybackEngine},
        output::OutputMode::{self, BitPerfect, Resampled},
    },
    storage::database::SqliteStorage,
    ui::player::{panel::format_time, queue::build_queue_view},
};

/// Build the playback control buttons (prev, play/pause, next).
///
/// Returns the button box and the play/pause button reference for event wiring.
#[must_use]
pub fn build_playback_controls(state: &Arc<AppState>) -> (Box, Button) {
    let controls = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .halign(Center)
        .build();

    let prev_button = Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Previous track")
        .build();
    prev_button.update_property(&[PropertyLabel("Previous track")]);
    let state_prev = Arc::clone(state);
    prev_button.connect_clicked(move |_| {
        if let Err(e) = state_prev.playback.previous_track() {
            error!(error = %e, "Failed to skip to previous track");
        }
    });
    controls.append(&prev_button);

    let play_button = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .css_classes(["suggested-action", "circular"])
        .tooltip_text("Play or pause")
        .build();
    play_button.update_property(&[PropertyLabel("Play or pause")]);
    let state_play = Arc::clone(state);
    play_button.connect_clicked(move |_| {
        if let Err(e) = state_play.playback.toggle_pause() {
            error!(error = %e, "Failed to toggle playback");
        }
    });
    controls.append(&play_button);

    let next_button = Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Next track")
        .build();
    next_button.update_property(&[PropertyLabel("Next track")]);
    let state_next = Arc::clone(state);
    next_button.connect_clicked(move |_| {
        if let Err(e) = state_next.playback.next_track() {
            error!(error = %e, "Failed to skip to next track");
        }
    });
    controls.append(&next_button);

    (controls, play_button)
}

/// Seek to the current scale position, clamped to track duration.
fn seek_to_scale_value(playback: &PlaybackEngine, scale: &Scale) {
    let s = playback.state();
    if s.duration_seconds <= 0.0 {
        return;
    }
    let value = scale.value();
    let position = (value / 100.0) * s.duration_seconds;
    if let Err(e) = playback.seek_to(position) {
        error!(error = %e, "Seek failed");
    }
}

/// Build the seek section with slider and time labels.
///
/// Returns the container, seek scale, current time label, and total time label.
#[must_use]
pub fn build_seek_section(state: &Arc<AppState>) -> (Box, Scale, Label, Label) {
    let seek_box = Box::builder().orientation(Vertical).spacing(4).build();
    let time_row = Box::builder().orientation(Horizontal).build();

    let current_time = Label::builder()
        .label("00:00")
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .hexpand(true)
        .build();
    current_time.update_property(&[PropertyLabel("Current playback position")]);
    time_row.append(&current_time);

    let total_time = Label::builder()
        .label("00:00")
        .css_classes(["dim-label", "caption"])
        .halign(End)
        .build();
    total_time.update_property(&[PropertyLabel("Total track duration")]);
    time_row.append(&total_time);

    let seek_scale = Scale::with_range(Horizontal, 0.0, 100.0, 1.0);
    seek_scale.set_draw_value(false);
    seek_scale.set_hexpand(true);
    seek_scale.set_can_focus(true);
    seek_scale.set_tooltip_text(Some("Seek through the track"));

    let is_seeking = Arc::clone(&state.is_seeking);
    let gesture = GestureClick::new();
    gesture.set_button(0);

    let seeking_press = Arc::clone(&is_seeking);
    gesture.connect_pressed(move |_, _, _, _| {
        seeking_press.store(true, Release);
    });

    let seeking_release = Arc::clone(&is_seeking);
    let playback_release = Arc::clone(&state.playback);
    let scale_release = seek_scale.clone();
    gesture.connect_released(move |_, _, _, _| {
        seeking_release.store(false, Release);
        seek_to_scale_value(&playback_release, &scale_release);
    });

    let seeking_unpaired = Arc::clone(&is_seeking);
    let playback_unpaired = Arc::clone(&state.playback);
    let scale_unpaired = seek_scale.clone();
    gesture.connect_unpaired_release(move |_, _, _, _, _| {
        seeking_unpaired.store(false, Release);
        seek_to_scale_value(&playback_unpaired, &scale_unpaired);
    });

    seek_scale.add_controller(gesture);

    let playback_prev = Arc::clone(&state.playback);
    let time_preview = current_time.clone();
    seek_scale.connect_change_value(move |_, _, value| {
        let s = playback_prev.state();
        if s.duration_seconds <= 0.0 {
            return Proceed;
        }
        let position = (value / 100.0) * s.duration_seconds;
        time_preview.set_label(&format_time(position));
        Proceed
    });

    seek_box.append(&seek_scale);
    seek_box.append(&time_row);
    (seek_box, seek_scale, current_time, total_time)
}

/// Persist a volume change to the storage backend.
async fn persist_volume(storage: Arc<SqliteStorage>, volume: f64) {
    if let Err(e) = storage.set_volume(volume).await {
        warn!(error = %e, "Failed to persist volume");
    }
}

/// Build the volume control section with an output-mode toggle button.
///
/// Returns the container box, the mode toggle button, and the volume scale
/// (for event-driven visual updates by the panel).
#[must_use]
pub fn build_volume_control(state: &Arc<AppState>) -> (Box, Button, Scale) {
    let vol_box = Box::builder().orientation(Horizontal).spacing(6).build();

    let mute_button = Button::builder()
        .icon_name("audio-volume-high-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Mute or unmute")
        .build();
    mute_button.update_property(&[PropertyLabel("Mute or unmute")]);
    let state_mute = Arc::clone(state);
    let mute_btn_ref = mute_button.clone();
    mute_button.connect_clicked(move |_| {
        let current = state_mute.playback.state();
        let new_muted = current.muted == Unmuted;
        if let Err(e) = state_mute.playback.set_muted(new_muted) {
            error!(error = %e, "Failed to set mute");
        }
        let icon = if new_muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        mute_btn_ref.set_icon_name(icon);
    });
    vol_box.append(&mute_button);

    let initial_volume = state.playback.state().volume;
    let volume_scale = Scale::with_range(Horizontal, 0.0, 1.0, 0.01);
    volume_scale.set_value(initial_volume);
    volume_scale.set_draw_value(false);
    volume_scale.set_hexpand(true);
    volume_scale.set_can_focus(true);
    let state_vol = Arc::clone(state);
    let vol_ref = volume_scale.clone();
    volume_scale.connect_value_changed(move |_| {
        let value = vol_ref.value();
        if let Err(e) = state_vol.playback.set_volume(value) {
            error!(error = %e, "Failed to set volume");
        }
        spawn_future_local(persist_volume(Arc::clone(&state_vol.storage), value));
    });
    vol_box.append(&volume_scale);

    let initial_mode = state.playback.state().output_mode;
    let mode_button = Button::builder()
        .icon_name(initial_mode.icon_name())
        .css_classes(["flat", "caption"])
        .tooltip_text(mode_button_tooltip(initial_mode))
        .build();
    let state_mode = Arc::clone(state);
    let scale_for_click = volume_scale.clone();
    mode_button.connect_clicked(move |btn| {
        let current_mode = state_mode.playback.state().output_mode;
        let new_mode = match current_mode {
            BitPerfect => Resampled,
            Resampled => BitPerfect,
        };
        if let Err(e) = state_mode.playback.set_output_mode(new_mode) {
            error!(error = %e, "Failed to toggle output mode");
        }
        btn.set_icon_name(new_mode.icon_name());
        btn.set_tooltip_text(Some(mode_button_tooltip(new_mode)));
        update_volume_scale_visual(&scale_for_click, new_mode);
    });
    vol_box.append(&mode_button);

    update_volume_scale_visual(&volume_scale, initial_mode);

    (vol_box, mode_button, volume_scale)
}

/// Update the volume scale's visual state based on the output mode.
///
/// In bit-perfect mode the scale is greyed out and interaction is
/// prevented — the volume is controlled via the ALSA hardware mixer.
pub fn update_volume_scale_visual(scale: &Scale, mode: OutputMode) {
    match mode {
        Resampled => {
            scale.set_sensitive(true);
            scale.set_tooltip_text(Some("Adjust volume"));
        }
        BitPerfect => {
            scale.set_sensitive(false);
            scale.set_tooltip_text(Some(
                "Volume controlled via hardware mixer \u{2014} switch to Resampled for software \
                 volume",
            ));
        }
    }
}

/// Tooltip text for the mode toggle button.
#[must_use]
pub fn mode_button_tooltip(mode: OutputMode) -> &'static str {
    match mode {
        BitPerfect => {
            "Bit-Perfect mode \u{2014} no software volume scaling, hardware volume via ALSA mixer"
        }
        Resampled => "Resampled mode \u{2014} software volume scaling, sample rate conversion",
    }
}

/// Build the queue section with label and queue view.
#[must_use]
pub fn build_queue_section(state: &Arc<AppState>) -> Box {
    let section = Box::builder().orientation(Vertical).spacing(4).build();

    let queue_label = Label::builder()
        .label("Queue")
        .css_classes(["heading", "dim-label"])
        .halign(Start)
        .build();
    queue_label.update_property(&[PropertyLabel("Playback queue section")]);
    section.append(&queue_label);

    let queue = state.playback.queue().clone();
    let queue_view = build_queue_view(state, &queue);
    section.append(&queue_view);

    section
}

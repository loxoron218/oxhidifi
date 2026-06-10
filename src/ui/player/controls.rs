//! Playback control widgets: transport buttons, seek slider, and volume control.

use std::sync::Arc;

use {
    libadwaita::{
        gtk::{
            Align::{Center, End, Start},
            Box, Button, Label,
            Orientation::{Horizontal, Vertical},
            Scale,
            accessible::Property::Label as PropertyLabel,
            prelude::RangeExt,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, ScaleExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::AppState, playback::engine::PlaybackController, ui::player::queue::build_queue_view,
};

/// Build the playback control buttons (prev, play/pause, next).
///
/// Returns the button box and the play/pause button reference for event wiring.
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

/// Build the seek section with slider and time labels.
///
/// Returns the container, seek scale, current time label, and total time label.
#[must_use]
pub fn build_seek_section() -> (Box, Scale, Label, Label) {
    let seek_box = Box::builder().orientation(Vertical).spacing(4).build();

    let seek_scale = Scale::with_range(Horizontal, 0.0, 100.0, 1.0);
    seek_scale.set_draw_value(false);
    seek_scale.set_hexpand(true);
    seek_scale.set_can_focus(true);
    seek_scale.set_tooltip_text(Some("Seek through the track"));
    seek_box.append(&seek_scale);

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

    seek_box.append(&time_row);
    (seek_box, seek_scale, current_time, total_time)
}

/// Build the volume control section.
pub fn build_volume_control(state: &Arc<AppState>) -> Box {
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
        let new_muted = !current.is_muted;
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

    let volume_scale = Scale::with_range(Horizontal, 0.0, 1.0, 0.01);
    volume_scale.set_draw_value(false);
    volume_scale.set_hexpand(true);
    volume_scale.set_can_focus(true);
    volume_scale.set_tooltip_text(Some("Adjust volume"));
    let state_vol = Arc::clone(state);
    let vol_ref = volume_scale.clone();
    volume_scale.connect_value_changed(move |_| {
        let value = vol_ref.value();
        if let Err(e) = state_vol.playback.set_volume(value) {
            error!(error = %e, "Failed to set volume");
        }
    });
    vol_box.append(&volume_scale);

    vol_box
}

/// Build the queue section with label and queue view.
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

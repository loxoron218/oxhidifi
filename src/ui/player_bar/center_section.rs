//! Center section of the player bar with playback controls.

use libadwaita::{
    gtk::{
        Align::Center,
        Box, Button, Label,
        Orientation::{Horizontal, Vertical},
        Scale, ToggleButton, Widget,
    },
    prelude::{BoxExt, ButtonExt, Cast, RangeExt, WidgetExt},
};

use crate::{
    audio::engine::{
        PlaybackState,
        PlaybackState::{Buffering, Paused, Playing, Ready, Stopped},
    },
    state::PlaybackQueue,
};

/// Creates the center section containing progress bar, time labels, and playback buttons.
///
/// # Returns
///
/// A tuple containing:
/// - The section container box
/// - The progress scale widget
/// - The current time label
/// - The total duration label
/// - The previous track button
/// - The play/pause toggle button
/// - The next track button
#[must_use]
pub fn create_center_section() -> (Box, Scale, Label, Label, Button, ToggleButton, Button) {
    let center_section = Box::builder()
        .orientation(Vertical)
        .hexpand(true)
        .vexpand(false)
        .valign(Center)
        .build();

    let progress_scale = Scale::builder()
        .orientation(Horizontal)
        .hexpand(true)
        .draw_value(false)
        .build();
    progress_scale.set_range(0.0, 100.0);
    center_section.append(progress_scale.upcast_ref::<Widget>());

    let time_row = Box::builder().orientation(Horizontal).hexpand(true).build();

    let current_time_label = Label::builder()
        .label("00:00")
        .css_classes(["dim-label", "numeric"])
        .build();
    time_row.append(current_time_label.upcast_ref::<Widget>());

    let spacer = Box::builder().orientation(Horizontal).hexpand(true).build();
    time_row.append(&spacer);

    let total_duration_label = Label::builder()
        .label("00:00")
        .css_classes(["dim-label", "numeric", "hifi-metadata-container"])
        .build();
    time_row.append(total_duration_label.upcast_ref::<Widget>());

    center_section.append(&time_row);

    let buttons_row = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .halign(Center)
        .hexpand(false)
        .build();

    let prev_button = Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .tooltip_text("Previous track")
        .use_underline(true)
        .has_frame(false)
        .sensitive(false)
        .build();
    buttons_row.append(prev_button.upcast_ref::<Widget>());

    let play_button = ToggleButton::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play")
        .use_underline(true)
        .has_frame(false)
        .sensitive(false)
        .build();
    buttons_row.append(play_button.upcast_ref::<Widget>());

    let next_button = Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .tooltip_text("Next track")
        .use_underline(true)
        .has_frame(false)
        .sensitive(false)
        .build();
    buttons_row.append(next_button.upcast_ref::<Widget>());

    center_section.append(&buttons_row);

    (
        center_section,
        progress_scale,
        current_time_label,
        total_duration_label,
        prev_button,
        play_button,
        next_button,
    )
}

/// Updates playback state display.
///
/// # Arguments
///
/// * `state` - Current playback state
/// * `play_button` - Play/pause toggle button
pub fn update_playback_state(state: &PlaybackState, play_button: &ToggleButton) {
    match state {
        Playing => {
            play_button.set_icon_name("media-playback-pause-symbolic");
            play_button.set_tooltip_text(Some("Pause"));
        }
        Paused | Stopped | Ready => {
            play_button.set_icon_name("media-playback-start-symbolic");
            play_button.set_tooltip_text(Some("Play"));
        }
        Buffering => {
            play_button.set_icon_name("media-playback-pause-symbolic");
            play_button.set_tooltip_text(Some("Buffering..."));
        }
    }
}

/// Updates prev/next button sensitivity based on queue state.
///
/// # Arguments
///
/// * `prev_button` - Previous track button
/// * `next_button` - Next track button
/// * `queue` - Playback queue reference
pub fn update_queue_buttons(prev_button: &Button, next_button: &Button, queue: &PlaybackQueue) {
    if queue.tracks.is_empty() {
        prev_button.set_sensitive(false);
        next_button.set_sensitive(false);
        return;
    }

    let current_index = queue.current_index.unwrap_or(0);
    let total_tracks = queue.tracks.len();

    prev_button.set_sensitive(current_index > 0);
    next_button.set_sensitive(current_index + 1 < total_tracks);
}

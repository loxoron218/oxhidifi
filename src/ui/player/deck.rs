//! Playback control widgets: transport buttons, seek slider, and queue section.

use std::sync::{Arc, atomic::Ordering::Release};

use {
    libadwaita::{
        glib::Propagation::Proceed,
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
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    playback::{engine::PlaybackEngine, transport::PlaybackTransport},
    ui::player::{playlist::build_queue_view, sidebar::format_time},
};

/// Build the playback control buttons (prev, play/pause, next).
///
/// Returns the button box and the play/pause button reference for event wiring.
pub fn build_transport(state: &Arc<AppState>) -> (Box, Button) {
    let controls = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .halign(Center)
        .build();

    let prev_button = Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Previous track")
        .can_focus(true)
        .build();
    prev_button.update_property(&[PropertyLabel("Previous track")]);
    let state_prev = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(prev_button.connect_clicked(move |_| {
            if let Err(e) = state_prev.playback.previous_track() {
                error!(error = %e, "Failed to skip to previous track");
            }
        }));
    controls.append(&prev_button);

    let play_button = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .css_classes(["suggested-action", "circular"])
        .tooltip_text("Play or pause")
        .can_focus(true)
        .build();
    play_button.update_property(&[PropertyLabel("Play or pause")]);
    let state_play = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(play_button.connect_clicked(move |_| {
            if let Err(e) = state_play.playback.toggle_pause() {
                error!(error = %e, "Failed to toggle playback");
            }
        }));
    controls.append(&play_button);

    let next_button = Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Next track")
        .can_focus(true)
        .build();
    next_button.update_property(&[PropertyLabel("Next track")]);
    let state_next = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(next_button.connect_clicked(move |_| {
            if let Err(e) = state_next.playback.next_track() {
                error!(error = %e, "Failed to skip to next track");
            }
        }));
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
    state
        .handles
        .lock()
        .retain_signal(gesture.connect_pressed(move |_, _, _, _| {
            seeking_press.store(true, Release);
        }));

    let seeking_release = Arc::clone(&is_seeking);
    let playback_release = Arc::clone(&state.playback);
    let scale_release = seek_scale.clone();
    state
        .handles
        .lock()
        .retain_signal(gesture.connect_released(move |_, _, _, _| {
            seeking_release.store(false, Release);
            seek_to_scale_value(&playback_release, &scale_release);
        }));

    let seeking_unpaired = Arc::clone(&is_seeking);
    let playback_unpaired = Arc::clone(&state.playback);
    let scale_unpaired = seek_scale.clone();
    state
        .handles
        .lock()
        .retain_signal(gesture.connect_unpaired_release(move |_, _, _, _, _| {
            seeking_unpaired.store(false, Release);
            seek_to_scale_value(&playback_unpaired, &scale_unpaired);
        }));

    seek_scale.update_property(&[PropertyLabel("Seek through the track")]);
    seek_scale.add_controller(gesture);

    let playback_prev = Arc::clone(&state.playback);
    let time_preview = current_time.clone();
    state
        .handles
        .lock()
        .retain_signal(seek_scale.connect_change_value(move |_, _, value| {
            let s = playback_prev.state();
            if s.duration_seconds <= 0.0 {
                return Proceed;
            }
            let position = (value / 100.0) * s.duration_seconds;
            time_preview.set_label(&format_time(position));
            Proceed
        }));

    seek_box.append(&seek_scale);
    seek_box.append(&time_row);
    (seek_box, seek_scale, current_time, total_time)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::object::ObjectExt,
            gtk::{self, prelude::RangeExt, test},
            prelude::{ButtonExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::player::deck::{build_queue_section, build_seek_section, build_transport},
    };

    #[test]
    fn transport_builds_three_buttons_with_play_icon() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (controls, play_button) = build_transport(&state);
        ensure!(
            play_button.icon_name().as_deref() == Some("media-playback-start-symbolic"),
            "play button must show the start icon"
        );
        let first = controls.first_child();
        let second = first.as_ref().and_then(WidgetExt::next_sibling);
        let third = second.as_ref().and_then(WidgetExt::next_sibling);
        let fourth = third.as_ref().and_then(WidgetExt::next_sibling);
        ensure!(
            first.is_some() && second.is_some() && third.is_some() && fourth.is_none(),
            "transport must hold exactly prev/play/next buttons"
        );
        Ok(())
    }

    #[test]
    fn seek_section_spans_full_range_with_zeroed_labels() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (_, scale, current, total) = build_seek_section(&state);
        let adjustment = scale.adjustment();
        let lower = adjustment.property::<f64>("lower");
        let upper = adjustment.property::<f64>("upper");
        ensure!(
            (lower - 0.0).abs() < f64::EPSILON && (upper - 100.0).abs() < f64::EPSILON,
            "seek scale must span 0..100"
        );
        ensure!(
            current.label() == "00:00",
            "current label must start zeroed"
        );
        ensure!(total.label() == "00:00", "total label must start zeroed");
        Ok(())
    }

    #[test]
    fn queue_section_installs_child() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let section = build_queue_section(&state);
        ensure!(
            section.first_child().is_some(),
            "queue section must install a child"
        );
        Ok(())
    }
}

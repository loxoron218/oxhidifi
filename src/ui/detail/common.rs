//! Common UI widgets and helpers for detail pages.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        gdk::Key,
        glib::{
            Propagation::{Proceed, Stop},
            spawn_future_local,
        },
        gtk::{
            Align::{End, Start},
            Box, Button, EventControllerKey, GestureClick, Label, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            ScrolledWindow,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End as EllipsizeEnd,
            prelude::{AccessibleExtManual, BoxExt, GestureSingleExt, ListBoxRowExt, WidgetExt},
        },
        prelude::ButtonExt,
    },
    num_traits::NumCast,
    tracing::{error, info},
};

use crate::{
    app::{
        AppState,
        NavigationEvent::{self, Back},
    },
    playback::engine::PlaybackController,
    storage::{Storage, Track, format_sample_rate_str},
};

/// Build the wrapper box with back navigation and header bar for a detail page.
#[must_use]
pub fn build_detail_wrapper(nav_tx: &Sender<NavigationEvent>, title: &str) -> Box {
    let wrapper = Box::builder().orientation(Vertical).can_focus(true).build();
    let back_button = setup_back_navigation(&wrapper, nav_tx.clone());
    let header_bar = build_detail_header(&back_button, title);
    wrapper.append(&header_bar);
    wrapper
}

/// Try to send a Back navigation event, logging on failure.
pub fn try_send_back(tx: &Sender<NavigationEvent>) {
    if let Err(e) = tx.try_send(Back) {
        error!(error = %e, "Failed to send Back navigation");
    }
}

/// Set up back button and Escape key navigation.
#[must_use]
pub fn setup_back_navigation(widget: &impl WidgetExt, nav_tx: Sender<NavigationEvent>) -> Button {
    let back_button = Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back to library")
        .css_classes(["flat"])
        .build();
    back_button.update_property(&[PropertyLabel("Back to library")]);

    let ntx = nav_tx.clone();
    back_button.connect_clicked(move |_| {
        try_send_back(&ntx);
    });

    let nav_back = nav_tx;
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == Key::Escape {
            try_send_back(&nav_back);
            Stop
        } else {
            Proceed
        }
    });
    widget.add_controller(key_controller);

    back_button
}

/// Build a scrollable content area with standard margins and spacing.
#[must_use]
pub fn build_scroll_content() -> (ScrolledWindow, Box) {
    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let content = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(18)
        .margin_end(18)
        .build();

    (scroll, content)
}

/// Build a header bar-like box with back button and title.
#[must_use]
pub fn build_detail_header(back_button: &Button, title: &str) -> Box {
    let header = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .css_classes(["toolbar"])
        .build();

    header.append(back_button);

    let title_label = Label::builder()
        .label(title)
        .css_classes(["title-4", "heading"])
        .hexpand(true)
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel(&format!("{title} detail page"))]);
    header.append(&title_label);

    header
}

/// Build a single track row with number, title, duration, and play/queue actions.
#[must_use]
pub fn build_track_row(
    state: &Arc<AppState>,
    track: &Track,
    display_number: usize,
    _nav_tx: &Sender<NavigationEvent>,
) -> ListBoxRow {
    let row = ListBoxRow::builder()
        .activatable(true)
        .tooltip_text("Click to play, right-click to add to queue")
        .build();

    let hbox = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();

    let number_label = Label::builder()
        .label(display_number.to_string())
        .width_request(30)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    number_label.update_property(&[PropertyLabel(&format!("Track {display_number}"))]);
    hbox.append(&number_label);

    let title_lbl = Label::builder()
        .label(&track.title)
        .ellipsize(EllipsizeEnd)
        .hexpand(true)
        .halign(Start)
        .build();
    title_lbl.update_property(&[PropertyLabel(&format!("Track: {}", track.title))]);
    hbox.append(&title_lbl);

    let track_format = track.audio.bit_depth.map_or_else(
        || {
            format!(
                "{} {}",
                track.audio.format,
                format_sample_rate_str(track.audio.sample_rate)
            )
        },
        |bd| {
            format!(
                "{} {bd}/{}",
                track.audio.format,
                format_sample_rate_str(track.audio.sample_rate)
            )
        },
    );
    let fmt_label = Label::builder()
        .label(&track_format)
        .css_classes(["dim-label", "caption"])
        .halign(End)
        .margin_start(12)
        .build();
    hbox.append(&fmt_label);

    let duration_label = Label::builder()
        .label(format_duration(track.duration))
        .css_classes(["dim-label", "caption"])
        .halign(End)
        .build();
    hbox.append(&duration_label);

    row.set_child(Some(&hbox));

    let sc = Arc::clone(state);
    let tid = track.id;
    let click = GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        spawn_playback(&sc, tid);
    });
    row.add_controller(click);

    let sc_kb = Arc::clone(state);
    let tid_kb = track.id;
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == Key::Return || key == Key::KP_Enter {
            spawn_playback(&sc_kb, tid_kb);
            Stop
        } else {
            Proceed
        }
    });
    row.add_controller(key_controller);

    let sc2 = Arc::clone(state);
    let tid2 = track.id;
    let right_click = GestureClick::new();
    right_click.set_button(3);
    right_click.connect_released(move |_, _, _, _| {
        sc2.playback.queue().append(tid2);
    });
    row.add_controller(right_click);

    row
}

/// Spawns playback of the track with the given ID.
fn spawn_playback(state: &Arc<AppState>, track_id: i64) {
    let state = Arc::clone(state);
    spawn_future_local(async move {
        play_single_track(&state, track_id).await;
    });
}

/// Play a track in its album context.
///
/// When the track belongs to an album, queues the entire album in
/// track-number order with the clicked track first. Otherwise plays
/// the track individually.
async fn play_single_track(state: &Arc<AppState>, track_id: i64) {
    let Ok(Some(track)) = state.storage.get_track(track_id).await else {
        info!(track_id, "Track not found");
        return;
    };

    let album_id = track.audio.album_id;
    let tracks = match album_id {
        Some(aid) => match state.storage.get_tracks_by_album(aid).await {
            Ok(t) => t,
            Err(e) => {
                info!(error = %e, album_id = aid, "Failed to fetch album tracks");
                vec![track]
            }
        },
        None => vec![track],
    };

    let clicked_idx = tracks.iter().position(|t| t.id == track_id).unwrap_or(0);
    let ordered: Vec<i64> = tracks[clicked_idx..]
        .iter()
        .chain(tracks[..clicked_idx].iter())
        .map(|t| t.id)
        .collect();

    let track_paths: HashMap<i64, PathBuf> = tracks
        .iter()
        .map(|t| (t.id, PathBuf::from(&t.audio.file_path)))
        .collect();
    state.playback.set_track_paths(track_paths);

    if let Err(e) = state.playback.play_queue(ordered) {
        info!(error = %e, track_id, "Failed to play track");
    }
}

/// Format seconds as `M:SS` or `MM:SS`.
#[must_use]
pub fn format_duration(seconds: f64) -> String {
    let total: u64 = NumCast::from(seconds.max(0.0)).unwrap_or(0);
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}

#[cfg(test)]
mod tests {
    use crate::ui::detail::common::format_duration;

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(0.0), "0:00");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(65.0), "1:05");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3661.0), "61:01");
    }

    #[test]
    fn format_duration_negative_treated_as_zero() {
        assert_eq!(format_duration(-5.0), "0:00");
    }
}

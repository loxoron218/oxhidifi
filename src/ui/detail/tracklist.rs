//! Track row widgets shared by album and artist detail pages.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use {
    crate::{
        app::runtime::AppState,
        playback::{queue_manager::PlaybackQueue, transport::PlaybackTransport},
        storage::{Storage, catalog::Track, formats::format_sample_rate_str},
    },
    async_channel::Sender,
    libadwaita::{
        gdk::Key,
        glib::{
            ControlFlow::{self, Break, Continue},
            Propagation::{Proceed, Stop},
            spawn_future_local,
        },
        gtk::{
            Align::{End, Start},
            Box, EventControllerKey, GestureClick, Label, ListBox, ListBoxRow,
            Orientation::Horizontal,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{AccessibleExtManual, BoxExt, GestureSingleExt, ListBoxRowExt, WidgetExt},
    },
    num_traits::NumCast,
    tracing::{info, warn},
};

/// Number of tracks to add per batch in the detail page track list.
const BATCH_SIZE: usize = 10;

/// Append up to [`BATCH_SIZE`] track rows from `remaining` to the list.
/// Call from an idle callback; returns `Continue` if more remain, `Break` when done.
pub fn fill_track_list_batch(
    remaining: &mut Vec<(Track, usize)>,
    track_list: &ListBox,
    state: &Arc<AppState>,
) -> ControlFlow {
    for _ in 0..BATCH_SIZE {
        let Some((track, display_num)) = remaining.pop() else {
            break;
        };
        let row = build_track_row(state, &track, display_num);
        track_list.append(&row);
    }
    if remaining.is_empty() {
        Break
    } else {
        Continue
    }
}

/// Build a single track row with number, title, duration, and play/queue actions.
pub fn build_track_row(state: &Arc<AppState>, track: &Track, display_number: usize) -> ListBoxRow {
    let row = ListBoxRow::builder()
        .activatable(true)
        .can_focus(true)
        .tooltip_text("Click to play, right-click to add to queue")
        .build();

    let hbox = build_track_content(track, display_number);
    row.set_child(Some(&hbox));
    row.update_property(&[PropertyLabel(&format!(
        "Track {display_number}: {}",
        track.title
    ))]);

    attach_track_controllers(&row, state, track.id);

    row
}

/// Build the hbox containing track number, title, format, and duration labels.
fn build_track_content(track: &Track, display_number: usize) -> Box {
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
        .width_chars(13)
        .xalign(0.0)
        .build();
    fmt_label.update_property(&[PropertyLabel(&format!(
        "Track {display_number} format: {track_format}"
    ))]);

    let duration_label = Label::builder()
        .label(format_duration(track.duration))
        .css_classes(["dim-label", "caption"])
        .width_chars(5)
        .xalign(0.0)
        .build();
    duration_label.update_property(&[PropertyLabel(&format!(
        "Track {display_number} duration: {}",
        format_duration(track.duration)
    ))]);

    let meta_box = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .halign(End)
        .margin_start(12)
        .build();
    meta_box.append(&fmt_label);
    meta_box.append(&duration_label);
    hbox.append(&meta_box);

    hbox
}

/// Attaches play-on-click, keyboard-play, and queue-on-right-click controllers.
fn attach_track_controllers(row: &ListBoxRow, state: &Arc<AppState>, track_id: i64) {
    let sc = Arc::clone(state);
    let tid = track_id;
    let click = GestureClick::new();
    state
        .handles
        .lock()
        .retain_signal(click.connect_released(move |_, _, _, _| {
            spawn_playback(&sc, tid);
        }));
    row.add_controller(click);

    let sc_kb = Arc::clone(state);
    let tid_kb = track_id;
    let key_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Return || key == Key::KP_Enter {
                spawn_playback(&sc_kb, tid_kb);
                Stop
            } else {
                Proceed
            }
        }));
    row.add_controller(key_controller);

    let sc2 = Arc::clone(state);
    let tid2 = track_id;
    let right_click = GestureClick::new();
    right_click.set_button(3);
    state
        .handles
        .lock()
        .retain_signal(right_click.connect_released(move |_, _, _, _| {
            if let Err(e) = sc2.playback.queue().append(tid2) {
                let msg = format!("Queue full (max {}): {e}", PlaybackQueue::MAX_CAPACITY);
                warn!(error = %e, "Queue append rejected — cap reached");
                send_queue_full_toast(&sc2.toast_tx, msg);
            }
        }));
    row.add_controller(right_click);
}

/// Send a queue-full toast, logging when the toast channel is full.
///
/// # Arguments
///
/// * `toast_tx` - Channel sender for toast notifications.
/// * `msg` - Toast message to display.
fn send_queue_full_toast(toast_tx: &Sender<String>, msg: String) {
    if let Err(e) = toast_tx.try_send(msg) {
        warn!(error = %e, "Queue-full toast dropped");
    }
}

/// Spawns playback of the track with the given ID.
fn spawn_playback(state: &Arc<AppState>, track_id: i64) {
    let state_cb = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            play_single_track(&state_cb, track_id).await;
        }));
}

/// Play a track in its album context.
///
/// When the track belongs to an album, queues the entire album in
/// track-number order with playback starting at the clicked track.
/// Otherwise plays the track individually.
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
                warn!(error = %e, album_id = aid, "Failed to fetch album tracks");
                vec![track]
            }
        },
        None => vec![track],
    };

    let clicked_idx = tracks.iter().position(|t| t.id == track_id).unwrap_or(0);
    let ordered: Vec<i64> = tracks.iter().map(|t| t.id).collect();

    let track_paths: HashMap<i64, PathBuf> = tracks
        .iter()
        .map(|t| (t.id, PathBuf::from(&t.audio.file_path)))
        .collect();
    state.playback.set_track_paths(track_paths);

    if let Err(e) = state.playback.play_at(ordered, clicked_idx) {
        warn!(error = %e, track_id, "Failed to play track");
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
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, test},
            prelude::{ListBoxRowExt, ListModelExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::catalog::{Track, TrackAudio},
        ui::detail::tracklist::{build_track_row, format_duration},
    };

    fn mock_track() -> Track {
        Track {
            id: 1,
            title: "Song".into(),
            number: Some(1),
            disc_number: Some(1),
            duration: 200.0,
            audio: TrackAudio {
                file_path: "/tmp/song.flac".into(),
                content_hash: None,
                format: "FLAC".into(),
                sample_rate: 96000,
                bit_depth: Some(24),
                channels: 2,
                codec: "FLAC".into(),
                lossless: true,
                bitrate: None,
                album_id: Some(1),
                artist_id: Some(1),
                file_size: 1000,
                last_modified: String::new(),
            },
            created_at: String::new(),
        }
    }

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

    #[test]
    fn build_track_row_attaches_controllers_and_content() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let row = build_track_row(&state, &mock_track(), 1);
        ensure!(row.observe_controllers().n_items() == 3);
        ensure!(row.child().is_some());
        Ok(())
    }
}

//! Player panel content with artwork, track info, and playback controls.
//!
//! Displays album artwork, track title, artist, seek slider, playback
//! controls, and volume slider. Used as the content of the sidebar pane.

use std::{sync::Arc, time::Duration};

use libadwaita::{
    glib::{
        ControlFlow::{Break, Continue},
        idle_add_local, spawn_future_local, timeout_add_local,
    },
    gtk::{
        Align::{Center, Start},
        Button,
        ContentFit::Cover,
        Label, Picture, ScrolledWindow,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End,
        prelude::RangeExt,
    },
    prelude::{AccessibleExtManual, BoxExt, ButtonExt},
};

use crate::{
    app::AppState,
    playback::{
        engine::{
            PlaybackController,
            PlaybackEvent::{self, Paused, Resumed, Stopped, TrackStarted},
            PlaybackState,
        },
        layout::{AudioLayout, format_channel_label},
    },
    storage::{Storage, database::SqliteStorage},
    ui::{
        detail::common::build_scroll_content,
        player::controls::{
            build_playback_controls, build_queue_section, build_seek_section, build_volume_control,
        },
    },
};

/// Format seconds into `MM:SS` display string.
#[must_use]
pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0).floor();
    let mins = (total / 60.0).floor();
    let secs = total - (mins * 60.0);
    format!("{mins:02.0}:{secs:02.0}")
}

/// Update the play/pause button icon based on playback state.
fn update_play_button(button: &Button, state: &PlaybackState) {
    let icon = if state.is_playing && !state.is_paused {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    button.set_icon_name(icon);
}

/// Set the play button icon based on a playback event.
fn set_play_icon_for_event(button: &Button, event: &PlaybackEvent) {
    let icon = match event {
        TrackStarted { .. } | Resumed => "media-playback-pause-symbolic",
        Paused | Stopped => "media-playback-start-symbolic",
        _ => return,
    };
    button.set_icon_name(icon);
}

/// Resolve track metadata from storage.
///
/// Returns `(title, artist_name, album_name, artwork_path, format_info)`.
async fn resolve_track_metadata(
    storage: &SqliteStorage,
    track_id: i64,
) -> (String, String, String, Option<String>, String) {
    let Ok(Some(track)) = storage.get_track(track_id).await else {
        return (
            format!("Track #{track_id}"),
            String::new(),
            String::new(),
            None,
            String::new(),
        );
    };

    let title = track.title;

    let artist_name = match track.audio.artist_id {
        Some(aid) => match storage.get_artist(aid).await {
            Ok(Some(a)) => a.name,
            _ => String::new(),
        },
        None => String::new(),
    };

    let (album_name, artwork_path) = match track.audio.album_id {
        Some(aid) => match storage.get_album(aid).await {
            Ok(Some(album)) => (album.title, album.artwork_path),
            _ => (String::new(), None),
        },
        None => (String::new(), None),
    };

    let channel_label = format_channel_label(AudioLayout::from_count(
        u32::try_from(track.audio.channels).unwrap_or(0),
    ));
    let sample_rate_khz = f64::from(track.audio.sample_rate) / 1000.0;
    let format_info = match track.audio.bit_depth {
        Some(depth) => {
            format!(
                "{} \u{2022} {depth}-bit / {sample_rate_khz:.1} kHz \u{2022} {channel_label}",
                track.audio.format,
            )
        }
        None => {
            format!(
                "{} \u{2022} {sample_rate_khz:.1} kHz \u{2022} {channel_label}",
                track.audio.format,
            )
        }
    };

    (title, artist_name, album_name, artwork_path, format_info)
}

/// Handle a `TrackStarted` event by resolving metadata and updating the UI.
async fn handle_track_event(
    storage: &Arc<SqliteStorage>,
    event: &PlaybackEvent,
    title_ref: &Label,
    artist_ref: &Label,
    album_ref: &Label,
    format_ref: &Label,
    artwork_ref: &Picture,
) {
    let TrackStarted { track_id } = event else {
        return;
    };

    let (title, resolved_artist, album_name, artwork_path, format_info) =
        resolve_track_metadata(storage, *track_id).await;

    let title_c = title_ref.clone();
    let artist_c = artist_ref.clone();
    let album_c = album_ref.clone();
    let format_c = format_ref.clone();
    let artwork_c = artwork_ref.clone();
    idle_add_local(move || {
        title_c.set_label(&title);
        artist_c.set_label(&resolved_artist);
        album_c.set_label(&album_name);
        format_c.set_label(&format_info);
        if let Some(path) = &artwork_path {
            artwork_c.set_filename(Some(path));
        }
        Break
    });
}

/// Build the player panel content area.
///
/// Returns a `ScrolledWindow` containing album artwork, track info,
/// seek slider, playback controls, volume control, and queue view.
/// Used as the content child of the sidebar's `AdwToolbarView`.
/// Subscribes to `PlaybackEvent` to update UI when tracks change.
#[must_use]
pub fn build_player_content(state: &Arc<AppState>) -> ScrolledWindow {
    let (scroll, content) = build_scroll_content();

    let artwork_image = build_artwork_placeholder();
    content.append(&artwork_image);

    let (title_label, artist_label, album_label, format_label) = build_track_info();
    content.append(&title_label);
    content.append(&artist_label);
    content.append(&album_label);
    content.append(&format_label);

    let (seek_section, seek_scale, current_time, total_time) = build_seek_section();
    content.append(&seek_section);
    let (controls_section, play_button) = build_playback_controls(state);
    content.append(&controls_section);
    content.append(&build_volume_control(state));
    content.append(&build_queue_section(state));

    scroll.set_child(Some(&content));

    let storage = Arc::clone(&state.storage);
    let title_ref = title_label;
    let artist_ref = artist_label;
    let album_ref = album_label;
    let format_ref = format_label;
    let artwork_ref = artwork_image;
    let play_btn_evt = play_button.clone();
    let state_event = Arc::clone(state);
    spawn_future_local(async move {
        let mut event_rx = state_event.playback.subscribe();
        while let Ok(event) = event_rx.recv().await {
            handle_track_event(
                &storage,
                &event,
                &title_ref,
                &artist_ref,
                &album_ref,
                &format_ref,
                &artwork_ref,
            )
            .await;
            set_play_icon_for_event(&play_btn_evt, &event);
        }
    });

    let playback = Arc::clone(&state.playback);
    let play_btn_poll = play_button;
    timeout_add_local(Duration::from_millis(200), move || {
        let s = playback.state();
        update_play_button(&play_btn_poll, &s);
        if s.is_playing || s.is_paused {
            let fraction = match s.duration_seconds {
                d if d > 0.0 => s.elapsed_seconds / d,
                _ => 0.0,
            };
            seek_scale.set_value(fraction * 100.0);
            current_time.set_label(&format_time(s.elapsed_seconds));
            total_time.set_label(&format_time(s.duration_seconds));
        }
        Continue
    });

    scroll
}

/// Build the album artwork placeholder.
fn build_artwork_placeholder() -> Picture {
    let artwork = Picture::builder()
        .content_fit(Cover)
        .can_shrink(true)
        .halign(Center)
        .width_request(280)
        .height_request(280)
        .css_classes(["album-cover"])
        .build();
    artwork.update_property(&[PropertyLabel("Album artwork")]);
    artwork
}

/// Build the track info section (title, artist, album, and format labels).
///
/// Returns the title, artist, album, and format `Label` widgets for dynamic updates.
fn build_track_info() -> (Label, Label, Label, Label) {
    let title = Label::builder()
        .label("No track playing")
        .css_classes(["title-3", "heading"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    title.update_property(&[PropertyLabel("Track title")]);

    let artist = Label::builder()
        .label("")
        .css_classes(["dim-label", "body"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    artist.update_property(&[PropertyLabel("Artist name")]);

    let album = Label::builder()
        .label("")
        .css_classes(["dim-label", "body"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    album.update_property(&[PropertyLabel("Album name")]);

    let format = Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    format.update_property(&[PropertyLabel("Audio format information")]);

    (title, artist, album, format)
}

#[cfg(test)]
mod tests {
    use crate::ui::player::panel::format_time;

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00");
    }

    #[test]
    fn format_time_minutes() {
        assert_eq!(format_time(90.0), "01:30");
    }

    #[test]
    fn format_time_hours() {
        assert_eq!(format_time(3661.0), "61:01");
    }
}

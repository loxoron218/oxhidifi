//! Player panel content with artwork, track info, and playback controls.
//!
//! Displays album artwork, track title, artist, seek slider, playback
//! controls, and volume slider. Used as the content of the sidebar pane.

use std::{
    sync::{
        Arc,
        atomic::Ordering::Acquire,
        mpsc::{Sender, channel},
    },
    thread::spawn,
    time::Duration,
};

use {
    libadwaita::{
        gdk::MemoryTexture,
        glib::{
            ControlFlow,
            ControlFlow::{Break, Continue},
            idle_add_local, timeout_add_local,
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
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, TextureExt},
    },
    parking_lot::Mutex,
    tokio::runtime::Runtime,
    tracing::error,
};

use crate::{
    app::AppState,
    playback::{
        engine::{
            PlaybackController, PlaybackState,
            PlaybackStatus::{Playing, Stopped as StatusStopped},
        },
        layout::{AudioLayout, format_channel_label},
    },
    storage::{Storage, database::SqliteStorage},
    ui::{
        CoverArtCache, DecodedCover, decode_cover_raw,
        detail::common::build_scroll_content,
        player::controls::{
            build_playback_controls, build_queue_section, build_seek_section, build_volume_control,
        },
        raw_to_texture,
    },
};

/// Minimum texture dimension (width or height) required to use a cached
/// cover in the player panel (displayed at 280×280).  Textures decoded
/// at 36 px by the column view are rejected, forcing a proper‑sized
/// decode via the async `handle_meta_update` path.
const COVER_MIN_SIZE: i32 = 180;

/// Tuple of resolved metadata: `(title, artist, album, artwork_path, format_info)`.
type MetaResult = (String, String, String, Option<String>, String);

/// Labels for track metadata display.
struct TrackLabels {
    /// Title label.
    title: Label,
    /// Artist label.
    artist: Label,
    /// Album label.
    album: Label,
    /// Format label.
    format: Label,
}

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
    let icon = if state.status == Playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    button.set_icon_name(icon);
}

/// Update track labels or spawn metadata fetch when track or status changes.
fn handle_status_change(
    is_stopped: bool,
    track_id: Option<i64>,
    labels: &TrackLabels,
    storage: &Arc<SqliteStorage>,
    meta_tx: &Sender<(i64, MetaResult)>,
) {
    if is_stopped {
        let t = labels.title.clone();
        let a = labels.artist.clone();
        let al = labels.album.clone();
        let f = labels.format.clone();
        idle_add_local(move || {
            t.set_label("No track playing");
            a.set_label("");
            al.set_label("");
            f.set_label("");
            Break
        });
        return;
    }
    let Some(track_id) = track_id else {
        return;
    };
    let storage = Arc::clone(storage);
    let tx = meta_tx.clone();
    spawn(move || {
        let Ok(rt) = Runtime::new() else {
            return;
        };
        let result = rt.block_on(resolve_track_metadata(&storage, track_id));
        if let Err(e) = tx.send((track_id, result)) {
            error!(error = %e, "Failed to send metadata");
        }
    });
}

/// Apply a resolved metadata result to the UI labels.
fn handle_meta_update(
    msg: (i64, MetaResult),
    current_track_id: Option<i64>,
    labels: &TrackLabels,
    cover_tx: &Sender<(i64, DecodedCover)>,
) {
    let (tid, (t, ar, al, art_path, fmt)) = msg;
    if Some(tid) != current_track_id {
        return;
    }
    let title = labels.title.clone();
    let artist = labels.artist.clone();
    let album = labels.album.clone();
    let format = labels.format.clone();
    idle_add_local(move || {
        title.set_label(&t);
        artist.set_label(&ar);
        album.set_label(&al);
        format.set_label(&fmt);
        Break
    });
    let Some(path) = art_path else {
        return;
    };
    let tx = cover_tx.clone();
    spawn(move || {
        let Some(decoded) = decode_cover_raw(&path, 280) else {
            return;
        };
        if let Err(e) = tx.send((tid, decoded)) {
            error!(error = %e, "Failed to send cover art");
        }
    });
}

/// Build a closure that sets cover artwork on the main thread.
fn set_cover_callback(artwork: Picture, texture: MemoryTexture) -> impl FnMut() -> ControlFlow {
    move || {
        artwork.set_paintable(Some(&texture));
        Break
    }
}

/// If `track_id` has a cached cover, dispatch a one-shot idle callback to
/// paint it onto `artwork`.  Called when the playback track or status
/// changes.
fn update_cover_for_track(track_id: Option<i64>, cover_cache: &CoverArtCache, artwork: &Picture) {
    if let Some(texture) = track_id.and_then(|tid| cover_cache.get(tid))
        && (texture.width() >= COVER_MIN_SIZE || texture.height() >= COVER_MIN_SIZE)
    {
        idle_add_local(set_cover_callback(artwork.clone(), (*texture).clone()));
    }
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

    let (seek_section, seek_scale, current_time, total_time) = build_seek_section(state);
    content.append(&seek_section);
    let (controls_section, play_button) = build_playback_controls(state);
    content.append(&controls_section);
    content.append(&build_volume_control(state));
    content.append(&build_queue_section(state));

    scroll.set_child(Some(&content));

    let poll_storage = Arc::clone(&state.storage);
    let poll_labels = TrackLabels {
        title: title_label,
        artist: artist_label,
        album: album_label,
        format: format_label,
    };
    let poll_artwork = artwork_image;
    let poll_playback = Arc::clone(&state.playback);
    let poll_btn = play_button;
    let mut prev_track_id = state.playback.state().current_track_id;
    let mut prev_status = state.playback.state().status;

    let (meta_tx, meta_rx) = channel::<(i64, MetaResult)>();
    let meta_rx = Mutex::new(meta_rx);

    let (cover_tx, cover_rx) = channel::<(i64, DecodedCover)>();
    let cover_rx = Mutex::new(cover_rx);

    let is_seeking = Arc::clone(&state.is_seeking);
    let cover_cache = Arc::clone(&state.cover_art_cache);

    timeout_add_local(Duration::from_millis(200), move || {
        let s = poll_playback.state();

        let changed = s.current_track_id != prev_track_id || s.status != prev_status;
        if changed {
            prev_track_id = s.current_track_id;
            prev_status = s.status;
        }

        if changed {
            update_cover_for_track(s.current_track_id, &cover_cache, &poll_artwork);

            handle_status_change(
                s.status == StatusStopped,
                s.current_track_id,
                &poll_labels,
                &poll_storage,
                &meta_tx,
            );
        }

        let guard = meta_rx.lock();
        while let Ok(msg) = guard.try_recv() {
            handle_meta_update(msg, s.current_track_id, &poll_labels, &cover_tx);
        }
        drop(guard);

        let cover_guard = cover_rx.lock();
        if let Ok((tid, cover)) = cover_guard.try_recv()
            && Some(tid) == s.current_track_id
        {
            let texture = raw_to_texture(&cover);
            cover_cache.insert(tid, texture.clone());
            idle_add_local(set_cover_callback(poll_artwork.clone(), texture));
        }
        drop(cover_guard);

        update_play_button(&poll_btn, &s);
        let playing = s.status != StatusStopped;
        if playing {
            total_time.set_label(&format_time(s.duration_seconds));
        }
        if playing && !is_seeking.load(Acquire) {
            let fraction = match s.duration_seconds {
                d if d > 0.0 => s.elapsed_seconds / d,
                _ => 0.0,
            };
            seek_scale.set_value(fraction * 100.0);
            current_time.set_label(&format_time(s.elapsed_seconds));
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

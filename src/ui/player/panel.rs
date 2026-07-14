//! Player panel content with artwork, track info, and playback controls.
//!
//! Displays album artwork, track title, artist, seek slider, playback
//! controls, and volume slider. Used as the content of the sidebar pane.
//! Subscribes to `PlaybackEvent` for fully event-driven updates.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Acquire},
};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        glib::{ControlFlow::Break, MainContext, idle_add_local},
        gtk::{
            Align::{Center, Start},
            Button,
            ContentFit::Cover,
            Label, Picture, Scale, ScrolledWindow,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
            prelude::RangeExt,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, TextureExt},
    },
    tokio::spawn,
    tracing::error,
};

use crate::{
    app::AppState,
    playback::{
        engine::{
            PlaybackController, PlaybackEngine,
            PlaybackEvent::{self, Paused, PositionTick, Resumed, Seeked, Stopped, TrackStarted},
        },
        layout::{AudioLayout, format_channel_label},
    },
    storage::{Storage, database::SqliteStorage},
    ui::{
        CoverArtCache, DecodedCover,
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
/// decode via the async metadata path.
const COVER_MIN_SIZE: i32 = 180;

/// Tuple of resolved metadata: `(title, artist, album, artwork_path, format_info)`.
type MetaResult = (String, String, String, Option<String>, String);

/// Widget references for playback control updates.
#[derive(Clone)]
struct PlaybackWidgets {
    /// Track title, artist, album, format labels.
    labels: TrackLabels,
    /// Album artwork display widget.
    artwork_image: Picture,
    /// Play/pause transport button.
    play_button: Button,
    /// Seek slider for position control.
    seek_scale: Scale,
    /// Label showing current playback position.
    current_time: Label,
    /// Label showing total track duration.
    total_time: Label,
}

/// Labels for track metadata display.
#[derive(Clone)]
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
    spawn(async move {
        let result = resolve_track_metadata(&storage, track_id).await;
        if tx.try_send((track_id, result)).is_err() {
            error!(target: "ui::player::panel", "Failed to send metadata");
        }
    });
}

/// If `track_id` has a cached cover, dispatch a one-shot idle callback to
/// paint it onto `artwork`.  Called when the playback track changes.
fn update_cover_from_cache(track_id: Option<i64>, cover_cache: &CoverArtCache, artwork: &Picture) {
    if let Some(texture) = track_id.and_then(|tid| cover_cache.get(tid))
        && (texture.width() >= COVER_MIN_SIZE || texture.height() >= COVER_MIN_SIZE)
    {
        let img = artwork.clone();
        let tex = (*texture).clone();
        idle_add_local(move || {
            img.set_paintable(Some(&tex));
            Break
        });
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
/// Listens to `PlaybackEvent` stream for fully event-driven updates.
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

    let widgets = PlaybackWidgets {
        labels: TrackLabels {
            title: title_label,
            artist: artist_label,
            album: album_label,
            format: format_label,
        },
        artwork_image,
        play_button,
        seek_scale,
        current_time,
        total_time,
    };

    spawn_async_listeners(state, widgets);
    scroll
}

/// Apply metadata labels to the UI via idle callback.
fn apply_meta_labels(labels: &TrackLabels, t: &str, ar: &str, al: &str, fmt: &str) {
    labels.title.set_label(t);
    labels.artist.set_label(ar);
    labels.album.set_label(al);
    labels.format.set_label(fmt);
}

/// Process one metadata update: check track ID match, update labels, request cover.
fn process_metadata(
    tid: i64,
    meta: MetaResult,
    widgets: &PlaybackWidgets,
    playback: &Arc<PlaybackEngine>,
    cover_cache: &Arc<CoverArtCache>,
    cover_tx: &Sender<(i64, DecodedCover)>,
    cover_size: i32,
) {
    if Some(tid) != playback.state().current_track_id {
        return;
    }
    let (t, ar, al, art_path, fmt) = meta;
    let labels = widgets.labels.clone();
    idle_add_local(move || {
        apply_meta_labels(&labels, &t, &ar, &al, &fmt);
        Break
    });
    if let Some(path) = art_path {
        cover_cache.request_decode_to_channel(
            tid,
            path,
            cover_size,
            cover_tx.clone(),
            "main thread",
        );
    }
}

/// Process one cover-art update: check track ID match, update texture.
fn process_cover_art(
    tid: i64,
    cover: &DecodedCover,
    widgets: &PlaybackWidgets,
    playback: &Arc<PlaybackEngine>,
    cover_cache: &Arc<CoverArtCache>,
) {
    if Some(tid) != playback.state().current_track_id {
        return;
    }
    let texture = raw_to_texture(cover);
    cover_cache.insert(tid, texture.clone());
    let img = widgets.artwork_image.clone();
    idle_add_local(move || {
        img.set_paintable(Some(&texture));
        Break
    });
}

/// Set up async listeners for playback events, metadata, and cover art.
fn spawn_async_listeners(state: &Arc<AppState>, widgets: PlaybackWidgets) {
    let (meta_tx, meta_rx) = unbounded::<(i64, MetaResult)>();
    let (cover_tx, cover_rx) = unbounded::<(i64, DecodedCover)>();

    let playback = Arc::clone(&state.playback);
    let is_seeking = Arc::clone(&state.is_seeking);
    let cover_cache = Arc::clone(&state.cover_art_cache);
    let storage = Arc::clone(&state.storage);

    let ev_rx = state.playback.subscribe();
    let w1 = widgets.clone();
    let p1 = Arc::clone(&playback);
    let s1 = Arc::clone(&is_seeking);
    let c1 = Arc::clone(&cover_cache);
    let st1 = Arc::clone(&storage);
    let mt1 = meta_tx;
    MainContext::default().spawn_local(async move {
        while let Ok(event) = ev_rx.recv().await {
            on_playback_event(&event, &w1, &p1, &s1, &c1, &st1, &mt1);
        }
    });

    let mw = widgets.clone();
    let mp = Arc::clone(&playback);
    let mc = Arc::clone(&cover_cache);
    MainContext::default().spawn_local(async move {
        while let Ok((tid, meta)) = meta_rx.recv().await {
            process_metadata(tid, meta, &mw, &mp, &mc, &cover_tx, 280);
        }
    });

    MainContext::default().spawn_local(async move {
        while let Ok((tid, cover)) = cover_rx.recv().await {
            process_cover_art(tid, &cover, &widgets, &playback, &cover_cache);
        }
    });
}

/// Handle a single playback event, updating UI widgets.
fn on_playback_event(
    event: &PlaybackEvent,
    widgets: &PlaybackWidgets,
    playback: &PlaybackEngine,
    is_seeking: &AtomicBool,
    cover_cache: &CoverArtCache,
    storage: &Arc<SqliteStorage>,
    meta_tx: &Sender<(i64, MetaResult)>,
) {
    match event {
        TrackStarted { track_id } => {
            update_cover_from_cache(Some(*track_id), cover_cache, &widgets.artwork_image);
            handle_status_change(false, Some(*track_id), &widgets.labels, storage, meta_tx);
            widgets
                .play_button
                .set_icon_name("media-playback-pause-symbolic");
        }
        Paused => {
            widgets
                .play_button
                .set_icon_name("media-playback-start-symbolic");
        }
        Resumed => {
            widgets
                .play_button
                .set_icon_name("media-playback-pause-symbolic");
        }
        Stopped => {
            widgets.labels.title.set_label("No track playing");
            widgets.labels.artist.set_label("");
            widgets.labels.album.set_label("");
            widgets.labels.format.set_label("");
            widgets
                .play_button
                .set_icon_name("media-playback-start-symbolic");
            widgets.total_time.set_label("00:00");
            if !is_seeking.load(Acquire) {
                widgets.seek_scale.set_value(0.0);
                widgets.current_time.set_label("00:00");
            }
        }
        Seeked { .. } => {
            let s = playback.state();
            if !is_seeking.load(Acquire) {
                let fraction = match s.duration_seconds {
                    d if d > 0.0 => s.elapsed_seconds / d,
                    _ => 0.0,
                };
                widgets.seek_scale.set_value(fraction * 100.0);
                widgets
                    .current_time
                    .set_label(&format_time(s.elapsed_seconds));
            }
            widgets
                .total_time
                .set_label(&format_time(s.duration_seconds));
        }
        PositionTick {
            elapsed_seconds,
            duration_seconds,
        } => {
            if !is_seeking.load(Acquire) {
                let fraction = match *duration_seconds {
                    d if d > 0.0 => *elapsed_seconds / d,
                    _ => 0.0,
                };
                widgets.seek_scale.set_value(fraction * 100.0);
                widgets
                    .current_time
                    .set_label(&format_time(*elapsed_seconds));
            }
            widgets
                .total_time
                .set_label(&format_time(*duration_seconds));
        }
        _ => {}
    }
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

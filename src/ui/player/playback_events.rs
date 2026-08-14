//! Event-driven updates for the player panel widgets.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Acquire},
};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gdk::MemoryTexture,
        glib::{ControlFlow::Break, MainContext, idle_add_local},
        gtk::{Picture, accessible::Property::Label as PropertyLabel},
        prelude::{AccessibleExtManual, ButtonExt, RangeExt, TextureExt, WidgetExt},
    },
    tokio::spawn,
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    playback::{
        engine::PlaybackEngine,
        layout::{AudioLayout, format_channel_label},
        state::PlaybackEvent::{
            self, OutputModeChanged, Paused, PositionTick, Resumed, Seeked, Stopped, TrackStarted,
        },
        transport::PlaybackTransport,
    },
    storage::{Storage, database::SqliteStorage},
    ui::{
        image_decode::{DecodedCover, raw_to_texture},
        player::{
            deck::{mode_button_tooltip, update_volume_scale_visual},
            sidebar::{COVER_MIN_SIZE, MetaResult, PlaybackWidgets, TrackLabels, format_time},
        },
        texture_pool::CoverArtCache,
    },
};

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
        if let Err(e) = tx.try_send((track_id, result)) {
            error!(error = %e, "Failed to send metadata");
        }
    });
}

/// If `track_id` has a cached cover large enough, paint it onto `artwork`
/// immediately.  Called when the playback track changes.
fn update_cover_from_cache(
    track_id: Option<i64>,
    cover_cache: &CoverArtCache,
    artwork: &Picture,
    playback: &PlaybackEngine,
) {
    let Some(tid) = track_id else { return };
    if playback.state().current_track_id != Some(tid) {
        return;
    }
    let Some(texture) = cover_cache.get_by_track(tid) else {
        return;
    };
    if texture.width() < COVER_MIN_SIZE && texture.height() < COVER_MIN_SIZE {
        return;
    }
    artwork.set_paintable(Some(&*texture));
}

/// Resolve track metadata from storage.
///
/// Returns `(title, artist_name, album_name, artwork_path, format_info, album_id)`.
async fn resolve_track_metadata(
    storage: &SqliteStorage,
    track_id: i64,
) -> (String, String, String, Option<String>, String, i64) {
    let Ok(Some(track)) = storage.get_track(track_id).await else {
        return (
            format!("Track #{track_id}"),
            String::new(),
            String::new(),
            None,
            String::new(),
            -1,
        );
    };

    let title = track.title;
    let album_id = track.audio.album_id.unwrap_or(-1);

    let artist_name = match track.audio.artist_id {
        Some(aid) => match storage.get_artist(aid).await {
            Ok(Some(a)) => a.name,
            _ => String::new(),
        },
        None => String::new(),
    };

    let (album_name, artwork_path) = match album_id {
        aid if aid >= 0 => match storage.get_album(aid).await {
            Ok(Some(album)) => (album.title, album.artwork_path),
            _ => (String::new(), None),
        },
        _ => (String::new(), None),
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

    (
        title,
        artist_name,
        album_name,
        artwork_path,
        format_info,
        album_id,
    )
}

/// Apply metadata labels to the UI via idle callback.
fn apply_meta_labels(labels: &TrackLabels, t: &str, ar: &str, al: &str, fmt: &str) {
    labels.title.set_label(t);
    labels.title.update_property(&[PropertyLabel(t)]);
    labels.artist.set_label(ar);
    labels.artist.update_property(&[PropertyLabel(ar)]);
    labels.album.set_label(al);
    labels.album.update_property(&[PropertyLabel(al)]);
    labels.format.set_label(fmt);
    labels.format.update_property(&[PropertyLabel(fmt)]);
}

/// Process one metadata update: check track ID match, update labels, request cover.
fn process_metadata(
    tid: i64,
    meta: MetaResult,
    widgets: &PlaybackWidgets,
    playback: &Arc<PlaybackEngine>,
    cover_cache: &Arc<CoverArtCache>,
    cover_tx: &Sender<(i64, i32, DecodedCover)>,
    cover_size: i32,
) {
    if Some(tid) != playback.state().current_track_id {
        return;
    }
    let (t, ar, al, art_path, fmt, album_id) = meta;
    apply_meta_labels(&widgets.labels, &t, &ar, &al, &fmt);

    if album_id >= 0 {
        cover_cache.record_track_album(tid, album_id);
        if let Some(texture) = cover_cache.get_any(album_id) {
            widgets.artwork_image.set_paintable(Some(&*texture));
            return;
        }
    }
    if let Some(path) = art_path {
        let key = if album_id >= 0 { album_id } else { tid };
        cover_cache.request_decode_to_channel(
            key,
            path,
            cover_size,
            cover_tx.clone(),
            "main thread",
        );
    }
}

/// Process one cover-art update: check staleness, update texture.
fn process_cover_art(
    aid: i64,
    size: i32,
    cover: &DecodedCover,
    widgets: &PlaybackWidgets,
    playback: &Arc<PlaybackEngine>,
    cover_cache: &Arc<CoverArtCache>,
) {
    let texture = raw_to_texture(cover);

    let Some(current_tid) = playback.state().current_track_id else {
        return;
    };

    let is_current = if aid >= 0 {
        cover_cache.get_album_for_track(current_tid) == Some(aid)
    } else {
        current_tid == aid
    };
    if !is_current {
        return;
    }

    if aid >= 0 {
        cover_cache.insert(aid, size, texture.clone());
    }

    widgets.artwork_image.set_paintable(Some(&texture));
}

/// Set up async listeners for playback events, metadata, and cover art.
pub fn spawn_async_listeners(state: &Arc<AppState>, widgets: PlaybackWidgets) {
    let (meta_tx, meta_rx) = unbounded::<(i64, MetaResult)>();
    let (cover_tx, cover_rx) = unbounded::<(i64, i32, DecodedCover)>();

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
        while let Ok((tid, size, cover)) = cover_rx.recv().await {
            process_cover_art(tid, size, &cover, &widgets, &playback, &cover_cache);
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
            widgets.artwork_image.set_paintable(None::<&MemoryTexture>);
            update_cover_from_cache(
                Some(*track_id),
                cover_cache,
                &widgets.artwork_image,
                playback,
            );
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
            widgets
                .labels
                .title
                .update_property(&[PropertyLabel("No track playing")]);
            widgets.labels.artist.set_label("");
            widgets.labels.artist.update_property(&[PropertyLabel("")]);
            widgets.labels.album.set_label("");
            widgets.labels.album.update_property(&[PropertyLabel("")]);
            widgets.labels.format.set_label("");
            widgets.labels.format.update_property(&[PropertyLabel("")]);
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
        OutputModeChanged { mode } => {
            widgets.output_mode_btn.set_icon_name(mode.icon_name());
            widgets
                .output_mode_btn
                .set_tooltip_text(Some(mode_button_tooltip(*mode)));
            widgets
                .output_mode_btn
                .update_property(&[PropertyLabel(mode_button_tooltip(*mode))]);
            update_volume_scale_visual(&widgets.volume_scale, *mode);
        }
        _ => {}
    }
}

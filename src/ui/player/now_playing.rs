//! Now-playing cover and metadata handling for the player panel.
//!
//! Resolves track metadata from storage off the main thread, paints cached
//! cover art when the track changes, and applies label and texture updates
//! for incoming metadata and cover-art messages.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        glib::idle_add_local_once,
        gtk::{Picture, accessible::Property::Label as PropertyLabel},
        prelude::{AccessibleExtManual, TextureExt},
    },
    tokio::spawn,
    tracing::error,
};

use crate::{
    playback::{
        engine::PlaybackEngine,
        layout::{AudioLayout, format_channel_label},
        transport::PlaybackTransport,
    },
    storage::{Storage, database::SqliteStorage},
    ui::{
        image_decode::{DecodedCover, raw_to_texture},
        player::sidebar::{COVER_MIN_SIZE, MetaResult, PlaybackWidgets, TrackLabels},
        signal_handlers::UiHandles,
        texture_pool::CoverArtCache,
    },
};

/// Update track labels or spawn metadata fetch when track or status changes.
///
/// # Arguments
///
/// * `is_stopped` - Whether playback has stopped; clears the labels when true.
/// * `track_id` - Track whose metadata should be resolved when playing.
/// * `labels` - Track label widgets to clear on stop.
/// * `storage` - Storage used to resolve the track metadata.
/// * `meta_tx` - Channel carrying the resolved metadata to the UI task.
///
/// # Returns
///
/// Nothing.
pub fn handle_status_change(
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
        let mut handles = UiHandles::default();
        handles.retain_source(idle_add_local_once(move || {
            t.set_label("No track playing");
            a.set_label("");
            al.set_label("");
            f.set_label("");
        }));
        return;
    }
    let Some(track_id) = track_id else {
        return;
    };
    let storage = Arc::clone(storage);
    let tx = meta_tx.clone();
    drop(spawn(async move {
        let result = resolve_track_metadata(&storage, track_id).await;
        if let Err(e) = tx.try_send((track_id, result)) {
            error!(error = %e, "Failed to send metadata");
        }
    }));
}

/// If `track_id` has a cached cover large enough, paint it onto `artwork`
/// immediately.  Called when the playback track changes.
///
/// # Arguments
///
/// * `track_id` - Track whose cached cover should be painted.
/// * `cover_cache` - Cache holding decoded cover textures.
/// * `artwork` - Picture widget receiving the cached texture.
/// * `playback` - Engine used to guard against stale track changes.
///
/// # Returns
///
/// Nothing.
pub fn update_cover_from_cache(
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
/// # Arguments
///
/// * `storage` - Storage holding the track, artist, and album rows.
/// * `track_id` - Track whose metadata should be resolved.
///
/// # Returns
///
/// `(title, artist_name, album_name, artwork_path, format_info, album_id)`.
pub async fn resolve_track_metadata(
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
///
/// # Arguments
///
/// * `labels` - Track label widgets receiving the metadata.
/// * `t` - Track title.
/// * `ar` - Artist name.
/// * `al` - Album title.
/// * `fmt` - Format info line.
///
/// # Returns
///
/// Nothing.
pub fn apply_meta_labels(labels: &TrackLabels, t: &str, ar: &str, al: &str, fmt: &str) {
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
///
/// # Arguments
///
/// * `tid` - Track the metadata belongs to.
/// * `meta` - Resolved metadata tuple for the track.
/// * `widgets` - Player widgets receiving labels and cover art.
/// * `playback` - Engine used to guard against stale updates.
/// * `cover_cache` - Cache consulted before requesting a decode.
/// * `cover_tx` - Channel carrying decode requests.
/// * `cover_size` - Requested cover decode size in pixels.
///
/// # Returns
///
/// Nothing.
pub fn process_metadata(
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
///
/// # Arguments
///
/// * `aid` - Album (or track fallback) ID the cover belongs to.
/// * `size` - Decode size of the cover in pixels.
/// * `cover` - Decoded cover bytes to turn into a texture.
/// * `widgets` - Player widgets receiving the cover texture.
/// * `playback` - Engine used to guard against stale updates.
/// * `cover_cache` - Cache storing the decoded texture.
///
/// # Returns
///
/// Nothing.
pub fn process_cover_art(
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

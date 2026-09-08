//! Album detail page orchestration: artwork, metadata, and track listing.

use std::{sync::Arc, time::Duration};

use {
    async_channel::Sender,
    libadwaita::{
        glib::{
            ControlFlow::{Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local, timeout_add_local,
        },
        gtk::{
            Widget,
            prelude::{BoxExt, ButtonExt, WidgetExt},
        },
    },
    tracing::{info, warn},
};

use crate::{
    app::runtime::{AppState, NavigationEvent},
    storage::{
        Storage,
        catalog::{Album, Track},
    },
    ui::{
        detail::{
            body_compose::{AlbumDetailWidgets, DETAIL_COVER_SIZE, build_album_content},
            cover_art::decode_cover_into_picture,
            page::build_detail_wrapper,
            tracklist::fill_track_list_batch,
        },
        gallery::play_action::{album_play_icon, toggle_or_play_album},
    },
};

/// Data loaded from storage for the album detail page.
///
/// Kept free of widget handles so the fetch future stays `Send`; the widgets
/// are populated separately via [`apply_album_detail`].
struct AlbumDetailData {
    /// The album record.
    album: Album,
    /// Resolved artist display name.
    artist_name: String,
    /// Detailed format summary text for the album.
    format_summary: String,
    /// Tracks belonging to the album, in display order.
    tracks: Vec<Track>,
}

/// Build the album detail page widget.
pub fn build_album_detail(
    state: &Arc<AppState>,
    album_id: i64,
    nav_tx: &Sender<NavigationEvent>,
) -> Widget {
    let wrapper = build_detail_wrapper(nav_tx, "Album");

    let content = build_album_content();
    wrapper.append(&content.scroll);

    content
        .play_button
        .set_icon_name(album_play_icon(state, album_id));
    let click_state = Arc::clone(state);
    let click_aid = album_id;
    let click_btn = content.play_button.clone();
    state
        .handles
        .lock()
        .retain_signal(content.play_button.connect_clicked(move |_| {
            let icon = album_play_icon(&click_state, click_aid);
            click_btn.set_icon_name(if icon == "media-playback-pause-symbolic" {
                "media-playback-start-symbolic"
            } else {
                "media-playback-pause-symbolic"
            });

            let s_cb = Arc::clone(&click_state);
            click_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    toggle_or_play_album(&s_cb, click_aid).await;
                }));
        }));

    let ev_btn = content.play_button.clone();
    let ev_state = Arc::clone(state);
    let ev_aid = album_id;
    let wrapper_alive = wrapper.clone();
    state
        .handles
        .lock()
        .retain_source(timeout_add_local(Duration::from_millis(200), move || {
            if wrapper_alive.parent().is_none() {
                return Break;
            }
            ev_btn.set_icon_name(album_play_icon(&ev_state, ev_aid));
            Continue
        }));

    let sc = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            if let Some(data) = fetch_album_detail(&sc, album_id).await {
                apply_album_detail(
                    &AlbumDetailWidgets {
                        artwork: &content.artwork,
                        title_label: &content.title_label,
                        artist_label: &content.artist_label,
                        year_label: &content.year_label,
                        genre_label: &content.genre_label,
                        tracks_label: &content.tracks_label,
                        format_label: &content.format_label,
                        track_list: &content.track_list,
                    },
                    &sc,
                    data,
                );
            }
        }));

    wrapper.upcast()
}

/// Load album detail data from storage.
///
/// Fetches the album record, artist name, format summary, and tracks.  Widget
/// updates are applied separately via [`apply_album_detail`], keeping this
/// future `Send`.
async fn fetch_album_detail(state: &Arc<AppState>, album_id: i64) -> Option<AlbumDetailData> {
    let album = match state.storage.get_album(album_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(album_id, "Album not found");
            return None;
        }
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album");
            return None;
        }
    };

    let artist_name = match state.storage.get_artist(album.artist_id).await {
        Ok(Some(a)) => a.name,
        _ => "Unknown Artist".to_string(),
    };

    let format_summary = state
        .storage
        .get_album_format_info(album_id)
        .await
        .unwrap_or_default()
        .summary_detailed();

    let tracks = match state.storage.get_tracks_by_album(album_id).await {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album tracks");
            return None;
        }
    };

    Some(AlbumDetailData {
        album,
        artist_name,
        format_summary,
        tracks,
    })
}

/// Apply album detail data to the detail page widgets.
fn apply_album_detail(
    widgets: &AlbumDetailWidgets<'_>,
    state: &Arc<AppState>,
    data: AlbumDetailData,
) {
    if let Some(path) = &data.album.artwork_path {
        decode_cover_into_picture(
            state,
            data.album.id,
            path.clone(),
            DETAIL_COVER_SIZE,
            widgets.artwork,
        );
    }

    widgets.title_label.set_label(&data.album.title);
    widgets.artist_label.set_label(&data.artist_name);

    if let Some(year) = data.album.year {
        widgets.year_label.set_label(&year.to_string());
        widgets.year_label.set_visible(true);
    } else {
        widgets.year_label.set_visible(false);
    }

    if let Some(genre) = &data.album.genre {
        widgets.genre_label.set_label(genre);
        widgets.genre_label.set_visible(true);
    } else {
        widgets.genre_label.set_visible(false);
    }

    widgets.tracks_label.set_label(&format!(
        "{} {}",
        data.album.track_count,
        if data.album.track_count == 1 {
            "track"
        } else {
            "tracks"
        },
    ));
    widgets.tracks_label.set_visible(true);

    if data.format_summary.is_empty() {
        widgets.format_label.set_visible(false);
    } else {
        widgets.format_label.set_label(&data.format_summary);
        widgets.format_label.set_visible(true);
    }

    let track_list = widgets.track_list.clone();
    let mut remaining: Vec<(Track, usize)> = data
        .tracks
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i.saturating_add(1)))
        .collect::<Vec<_>>();
    remaining.reverse();

    let state_cb = Arc::clone(state);
    state.handles.lock().retain_source(idle_add_local(move || {
        fill_track_list_batch(&mut remaining, &track_list, &state_cb)
    }));
}

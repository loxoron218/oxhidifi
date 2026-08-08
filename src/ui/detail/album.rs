//! Album detail page orchestration: artwork, metadata, and track listing.

use std::{boxed::Box, sync::Arc, time::Duration};

use {
    async_channel::{
        Receiver, Sender,
        TryRecvError::{Closed, Empty},
        unbounded,
    },
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local, timeout_add_local,
        },
        gtk::{
            Picture, Widget,
            prelude::{BoxExt, ButtonExt, WidgetExt},
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::{AppState, NavigationEvent},
    storage::{
        Storage,
        records::{Album, Track},
    },
    ui::{
        ArtworkDecodeRequest, DecodedCover,
        detail::{
            album_widgets::{AlbumDetailWidgets, DETAIL_COVER_SIZE, build_album_content},
            common::{build_detail_wrapper, fill_track_list_batch},
        },
        library::album_playback::{album_play_icon, toggle_or_play_album},
        raw_to_texture,
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
    /// Receiver for the decoded cover artwork, if the album has embedded art.
    cover_rx: Option<Receiver<DecodedCover>>,
}

/// Build the album detail page widget.
#[must_use]
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
    content.play_button.connect_clicked(move |_| {
        let icon = album_play_icon(&click_state, click_aid);
        click_btn.set_icon_name(if icon == "media-playback-pause-symbolic" {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });

        let s = Arc::clone(&click_state);
        let aid = click_aid;
        spawn_future_local(async move {
            toggle_or_play_album(&s, aid).await;
        });
    });

    let ev_btn = content.play_button.clone();
    let ev_state = Arc::clone(state);
    let ev_aid = album_id;
    let wrapper_alive = wrapper.clone();
    timeout_add_local(Duration::from_millis(200), move || {
        if wrapper_alive.parent().is_none() {
            return Break;
        }
        ev_btn.set_icon_name(album_play_icon(&ev_state, ev_aid));
        Continue
    });

    let sc = Arc::clone(state);
    spawn_future_local(async move {
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
    });

    wrapper.upcast()
}

/// Poll for decoded artwork and apply it to the picture widget.
fn poll_artwork(rx: &Receiver<DecodedCover>, artwork: &Picture) -> ControlFlow {
    match rx.try_recv() {
        Ok(decoded) => {
            let texture = raw_to_texture(&decoded);
            artwork.set_paintable(Some(&texture));
            Break
        }
        Err(Empty) => Continue,
        Err(Closed) => Break,
    }
}

/// Try to send decoded cover to the main thread channel, logging on failure.
fn try_send_cover(tx: &Sender<DecodedCover>, decoded: Option<DecodedCover>) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send(decoded) {
        error!(error = %e, "Failed to send decoded album detail cover to main thread");
    }
}

/// Load album detail data from storage.
///
/// Fetches the album record, artist name, format summary, and tracks, and
/// issues an artwork decode request.  Widget updates are applied separately
/// via [`apply_album_detail`], keeping this future `Send`.
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

    let cover_rx = album.artwork_path.as_ref().map(|path| {
        let path = path.clone();
        let (tx, rx) = unbounded::<DecodedCover>();

        state.cover_art_cache.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size: DETAIL_COVER_SIZE,
            on_complete: Box::new(move |_, decoded| try_send_cover(&tx, decoded)),
        });

        rx
    });

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
        cover_rx,
    })
}

/// Apply album detail data to the detail page widgets.
fn apply_album_detail(
    widgets: &AlbumDetailWidgets<'_>,
    state: &Arc<AppState>,
    data: AlbumDetailData,
) {
    if let Some(rx) = data.cover_rx {
        let artwork = widgets.artwork.clone();
        idle_add_local(move || poll_artwork(&rx, &artwork));
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

    let state = Arc::clone(state);
    idle_add_local(move || fill_track_list_batch(&mut remaining, &track_list, &state));
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, anyhow, ensure},
        async_channel::unbounded,
        libadwaita::{
            glib::ControlFlow::{Break, Continue},
            gtk::{self, Picture, test},
        },
    };

    use crate::ui::{
        DecodedCover,
        detail::album::{poll_artwork, try_send_cover},
        tests::{cover_send_forwards_decoded, cover_send_none_is_noop, mock_decoded_cover},
    };

    #[test]
    fn try_send_cover_none_is_noop() -> Result<()> {
        cover_send_none_is_noop(try_send_cover)
    }

    #[test]
    fn try_send_cover_forwards_decoded() -> Result<()> {
        cover_send_forwards_decoded(try_send_cover)
    }

    #[test]
    fn poll_artwork_breaks_on_decoded_cover() -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        let artwork = Picture::new();
        tx.try_send(mock_decoded_cover())
            .map_err(|e| anyhow!("{e}"))?;
        ensure!(poll_artwork(&rx, &artwork) == Break);
        Ok(())
    }

    #[test]
    fn poll_artwork_continues_while_waiting() -> Result<()> {
        let (_tx, rx) = unbounded::<DecodedCover>();
        let artwork = Picture::new();
        ensure!(poll_artwork(&rx, &artwork) == Continue);
        Ok(())
    }

    #[test]
    fn poll_artwork_breaks_on_closed_channel() -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        drop(tx);
        let artwork = Picture::new();
        ensure!(poll_artwork(&rx, &artwork) == Break);
        Ok(())
    }
}

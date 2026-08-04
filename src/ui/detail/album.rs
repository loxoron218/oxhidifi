//! Album detail page orchestration: artwork, metadata, and track listing.

use std::{boxed::Box, sync::Arc};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            MainContext, idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{
            Button, Picture, Widget,
            prelude::{BoxExt, ButtonExt, WidgetExt},
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::{AppState, NavigationEvent},
    playback::control::PlaybackController,
    storage::{Storage, records::Track},
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

    let ev_rx = state.playback.subscribe();
    let ev_btn = content.play_button.clone();
    let ev_state = Arc::clone(state);
    let ev_aid = album_id;
    MainContext::default().spawn_local(async move {
        while ev_rx.recv().await.is_ok() {
            let icon = album_play_icon(&ev_state, ev_aid);
            let btn = ev_btn.clone();
            idle_add_local(move || update_detail_play_button(&btn, icon));
        }
    });

    let sc = Arc::clone(state);
    spawn_future_local(async move {
        populate_album_detail(
            &sc,
            album_id,
            AlbumDetailWidgets {
                artwork: &content.artwork,
                title_label: &content.title_label,
                artist_label: &content.artist_label,
                year_label: &content.year_label,
                genre_label: &content.genre_label,
                tracks_label: &content.tracks_label,
                format_label: &content.format_label,
                track_list: &content.track_list,
            },
        )
        .await;
    });

    wrapper.upcast()
}

/// Update the detail page play button icon via idle callback.
fn update_detail_play_button(btn: &Button, icon: &'static str) -> ControlFlow {
    btn.set_icon_name(icon);
    Break
}

/// Try to send decoded cover to the main thread channel, logging on failure.
fn try_send_cover(tx: &Sender<DecodedCover>, decoded: Option<DecodedCover>) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send(decoded) {
        error!(error = %e, "Failed to send decoded album detail cover to main thread");
    }
}

/// Poll for decoded artwork and apply it to the picture widget.
fn poll_artwork(rx: &Receiver<DecodedCover>, artwork: &Picture) -> ControlFlow {
    rx.try_recv().map_or(Continue, |decoded| {
        let texture = raw_to_texture(&decoded);
        artwork.set_paintable(Some(&texture));
        Break
    })
}

/// Load album data from storage and populate the detail UI elements.
async fn populate_album_detail(
    state: &Arc<AppState>,
    album_id: i64,
    widgets: AlbumDetailWidgets<'_>,
) {
    let album = match state.storage.get_album(album_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(album_id, "Album not found");
            return;
        }
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album");
            return;
        }
    };

    if let Some(path) = &album.artwork_path {
        let path = path.clone();
        let (tx, rx) = unbounded::<DecodedCover>();

        state.cover_art_cache.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size: DETAIL_COVER_SIZE,
            on_complete: Box::new(move |_, decoded| try_send_cover(&tx, decoded)),
        });

        let artwork = widgets.artwork.clone();
        idle_add_local(move || poll_artwork(&rx, &artwork));
    }

    widgets.title_label.set_label(&album.title);

    let artist_name = match state.storage.get_artist(album.artist_id).await {
        Ok(Some(a)) => a.name,
        _ => "Unknown Artist".to_string(),
    };
    widgets.artist_label.set_label(&artist_name);

    if let Some(year) = album.year {
        widgets.year_label.set_label(&year.to_string());
        widgets.year_label.set_visible(true);
    } else {
        widgets.year_label.set_visible(false);
    }

    if let Some(genre) = &album.genre {
        widgets.genre_label.set_label(genre);
        widgets.genre_label.set_visible(true);
    } else {
        widgets.genre_label.set_visible(false);
    }

    widgets.tracks_label.set_label(&format!(
        "{} {}",
        album.track_count,
        if album.track_count == 1 {
            "track"
        } else {
            "tracks"
        },
    ));
    widgets.tracks_label.set_visible(true);

    let format_info = state
        .storage
        .get_album_format_info(album_id)
        .await
        .unwrap_or_default();
    let summary = format_info.summary_detailed();
    if summary.is_empty() {
        widgets.format_label.set_visible(false);
    } else {
        widgets.format_label.set_label(&summary);
        widgets.format_label.set_visible(true);
    }

    let tracks = match state.storage.get_tracks_by_album(album_id).await {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album tracks");
            return;
        }
    };

    let track_list = widgets.track_list.clone();
    let mut remaining: Vec<(Track, usize)> = tracks
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i + 1))
        .collect::<Vec<_>>();
    remaining.reverse();

    let state = Arc::clone(state);
    idle_add_local(move || fill_track_list_batch(&mut remaining, &track_list, &state));
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::ControlFlow::Break,
            gtk::{self, Button, test},
            prelude::ButtonExt,
        },
    };

    use crate::ui::{
        detail::album::{try_send_cover, update_detail_play_button},
        tests::{cover_send_forwards_decoded, cover_send_none_is_noop},
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
    fn update_detail_play_button_sets_icon() -> Result<()> {
        let button = Button::new();
        let flow = update_detail_play_button(&button, "media-playback-pause-symbolic");
        ensure!(flow == Break);
        ensure!(button.icon_name().as_deref() == Some("media-playback-pause-symbolic"));
        Ok(())
    }
}

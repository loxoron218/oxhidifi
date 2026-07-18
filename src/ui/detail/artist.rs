//! Artist detail page with album groupings and track listings.

use std::{boxed::Box, collections::HashMap, sync::Arc};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{
            Align::Start,
            Box as GtkBox,
            ContentFit::Cover,
            Label, ListBox, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            Picture, Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
            prelude::{AccessibleExtManual, BoxExt},
        },
    },
    tokio::join,
    tracing::{error, info, warn},
};

use crate::{
    app::{AppState, NavigationEvent},
    storage::{Album, FormatInfo, Storage, Track},
    ui::{
        ArtworkDecodeRequest, DecodedCover,
        detail::common::{build_detail_wrapper, build_scroll_content, fill_track_list_batch},
        raw_to_texture,
    },
};

/// Build the artist detail page widget.
#[must_use]
pub fn build_artist_detail(
    state: &Arc<AppState>,
    artist_id: i64,
    nav_tx: &Sender<NavigationEvent>,
) -> Widget {
    let wrapper = build_detail_wrapper(nav_tx, "Artist");

    let (scroll, content) = build_scroll_content();

    let name_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(End)
        .halign(Start)
        .build();
    name_label.update_property(&[PropertyLabel("Artist name")]);
    content.append(&name_label);

    let album_count_label = Label::builder()
        .css_classes(["dim-label", "body"])
        .halign(Start)
        .build();
    album_count_label.update_property(&[PropertyLabel("Album count")]);
    content.append(&album_count_label);

    let albums_container = GtkBox::builder().orientation(Vertical).spacing(18).build();
    content.append(&albums_container);

    scroll.set_child(Some(&content));
    wrapper.append(&scroll);

    let sc = Arc::clone(state);
    spawn_future_local(async move {
        populate_artist_detail(
            &sc,
            artist_id,
            &name_label,
            &album_count_label,
            &albums_container,
        )
        .await;
    });

    wrapper.upcast()
}

/// Load artist data from storage and populate the detail UI.
async fn populate_artist_detail(
    state: &Arc<AppState>,
    artist_id: i64,
    name_label: &Label,
    album_count_label: &Label,
    albums_container: &GtkBox,
) {
    let artist = match state.storage.get_artist(artist_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(artist_id, "Artist not found");
            return;
        }
        Err(e) => {
            warn!(error = %e, artist_id, "Failed to load artist");
            return;
        }
    };

    name_label.set_label(&artist.name);
    album_count_label.set_label(&format!("{} albums", artist.album_count));

    let albums = match state.storage.get_albums_by_artist(artist_id).await {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, artist_id, "Failed to load artist albums");
            return;
        }
    };

    let album_ids: Vec<i64> = albums.iter().map(|a| a.id).collect();
    let (format_info_map, all_tracks) = join!(
        state.storage.get_albums_format_info(&album_ids),
        state.storage.get_tracks_by_albums(&album_ids),
    );
    let format_info_map = format_info_map.unwrap_or_default();
    let all_tracks = all_tracks.unwrap_or_default();

    let mut tracks_by_album: HashMap<i64, Vec<Track>> = HashMap::new();
    for track in &all_tracks {
        tracks_by_album
            .entry(track.audio.album_id.unwrap_or(0))
            .or_default()
            .push(track.clone());
    }

    let mut track_lists: Vec<ListBox> = Vec::new();
    for album in &albums {
        let fi = format_info_map.get(&album.id).cloned().unwrap_or_default();
        let tracks = tracks_by_album.remove(&album.id).unwrap_or_default();
        let (section, listbox) = build_album_section(state, album, &fi, tracks);
        track_lists.push(listbox);
        albums_container.append(&section);
    }

    for (i, tb) in track_lists.iter().enumerate() {
        let others: Vec<ListBox> = track_lists
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, lb)| lb.clone())
            .collect();
        tb.connect_row_selected(move |_, row| clear_other_lists(row, &others));
    }
}

/// When a row is selected in one album's track list, unselect all rows
/// in the other albums' track lists to keep a single active highlight.
fn clear_other_lists(row: Option<&ListBoxRow>, others: &[ListBox]) {
    if row.is_none() {
        return;
    }
    for other in others {
        other.unselect_all();
    }
}

/// Build a section for a single album with its tracks.
///
/// Returns the section widget and the track list box for selection management.
/// Cover art is loaded asynchronously off the main thread.
/// Try to send decoded cover to the main thread channel, logging on failure.
fn try_send_artist_cover(tx: &Sender<DecodedCover>, decoded: Option<DecodedCover>) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send(decoded) {
        error!(error = %e, "Failed to send decoded artist cover to main thread");
    }
}

/// Poll for decoded artwork and apply it to the thumb picture widget.
fn poll_artist_artwork(rx: &Receiver<DecodedCover>, thumb: &Picture) -> ControlFlow {
    rx.try_recv().map_or(Continue, |decoded| {
        let texture = raw_to_texture(&decoded);
        thumb.set_paintable(Some(&texture));
        Break
    })
}

/// Build a section for a single album in the artist detail page.
///
/// Creates a header with thumbnail, title, metadata and a track list.
///
/// Returns the section widget and the track list box for selection management.
/// Cover art is loaded asynchronously off the main thread.
fn build_album_section(
    state: &Arc<AppState>,
    album: &Album,
    format_info: &FormatInfo,
    tracks: Vec<Track>,
) -> (GtkBox, ListBox) {
    let section = GtkBox::builder().orientation(Vertical).spacing(6).build();

    let album_header = GtkBox::builder()
        .orientation(Horizontal)
        .spacing(12)
        .build();

    if let Some(art_path) = &album.artwork_path {
        let thumb = Picture::builder()
            .content_fit(Cover)
            .can_shrink(true)
            .width_request(60)
            .height_request(60)
            .css_classes(["album-cover"])
            .build();
        thumb.update_property(&[PropertyLabel(&format!("Artwork for {}", album.title))]);
        album_header.append(&thumb);

        let path = art_path.clone();
        let (tx, rx) = unbounded::<DecodedCover>();

        state.cover_art_cache.request_decode(ArtworkDecodeRequest {
            album_id: album.id,
            path,
            size: 60,
            on_complete: Box::new(move |_, decoded| try_send_artist_cover(&tx, decoded)),
        });

        idle_add_local(move || poll_artist_artwork(&rx, &thumb));
    }

    let info_box = GtkBox::builder()
        .orientation(Vertical)
        .spacing(3)
        .hexpand(true)
        .build();

    let album_title = Label::builder()
        .label(&album.title)
        .css_classes(["title-4", "heading"])
        .ellipsize(End)
        .halign(Start)
        .build();
    album_title.update_property(&[PropertyLabel(&format!("Album: {}", album.title))]);
    info_box.append(&album_title);

    let album_meta = Label::builder()
        .label(format!(
            "{} tracks \u{2022} {}",
            album.track_count,
            format_info.summary_detailed()
        ))
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    album_meta.update_property(&[PropertyLabel(&format!(
        "{} tracks in {}",
        album.track_count, album.title
    ))]);
    info_box.append(&album_meta);

    album_header.append(&info_box);
    section.append(&album_header);

    let track_list = ListBox::builder().css_classes(["boxed-list"]).build();

    let mut remaining_tracks: Vec<(Track, usize)> = tracks
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i + 1))
        .collect();
    remaining_tracks.reverse();

    let tl = track_list.clone();
    let state = Arc::clone(state);
    idle_add_local(move || fill_track_list_batch(&mut remaining_tracks, &tl, &state));

    section.append(&track_list);
    (section, track_list)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::app::AppState;

    #[test]
    #[ignore = "Requires GTK initialization (display server)"]
    fn artist_detail_builds_with_state() -> Result<()> {
        AppState::mock()?;
        Ok(())
    }
}

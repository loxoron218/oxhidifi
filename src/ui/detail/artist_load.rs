//! Artist detail data loading and widget population.
//!
//! Split from [`artist_page`](super::artist_page) to keep each module under
//! the file-size limit. Fetching groups tracks by album off the main thread,
//! then populates the detail widgets on the main thread.

use std::{collections::HashMap, sync::Arc};

use {
    libadwaita::{
        gtk::{Box, Label, ListBox, ToggleButton},
        prelude::BoxExt,
    },
    tokio::join,
    tracing::{info, warn},
};

use crate::{
    app::runtime::AppState,
    storage::{
        Storage,
        catalog::{Album, Track},
        formats::FormatInfo,
    },
    ui::detail::{
        artist_actions::set_collapse_all_state, artist_album_section::build_album_section,
        artist_page::clear_other_lists,
    },
};

/// Data loaded from storage for the artist detail page.
///
/// Kept free of widget handles so the fetch future stays `Send`; the widgets
/// are populated separately via [`apply_artist_detail`].
#[derive(Debug)]
pub struct ArtistDetailData {
    /// Artist display name.
    name: String,
    /// Number of albums in the artist's discography.
    album_count: i32,
    /// Albums by this artist, in display order.
    albums: Vec<Album>,
    /// Per-album format info keyed by album ID.
    format_info_map: HashMap<i64, FormatInfo>,
    /// Per-album tracks keyed by album ID.
    tracks_by_album: HashMap<i64, Vec<Track>>,
}

/// Load artist detail data from storage.
///
/// Fetches the artist record, albums, format info, and tracks. Widget updates
/// are applied separately via [`apply_artist_detail`], keeping this future
/// `Send`.
///
/// # Arguments
///
/// * `state` - Application state owning storage.
/// * `artist_id` - Artist to load.
///
/// # Returns
///
/// * `Option<ArtistDetailData>` - Loaded data, or `None` when the artist or its albums fail to
///   load.
pub async fn fetch_artist_detail(
    state: &Arc<AppState>,
    artist_id: i64,
) -> Option<ArtistDetailData> {
    let artist = match state.storage.get_artist(artist_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(artist_id, "Artist not found");
            return None;
        }
        Err(e) => {
            warn!(error = %e, artist_id, "Failed to load artist");
            return None;
        }
    };

    let albums = match state.storage.get_albums_by_artist(artist_id).await {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, artist_id, "Failed to load artist albums");
            return None;
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

    Some(ArtistDetailData {
        name: artist.name,
        album_count: artist.album_count,
        albums,
        format_info_map,
        tracks_by_album,
    })
}

/// Apply artist detail data to the detail page widgets.
///
/// # Arguments
///
/// * `name_label` - Label receiving the artist name.
/// * `album_count_label` - Label receiving the album count.
/// * `albums_container` - Container receiving one section per album.
/// * `collapse_button` - Collapse-all toggle synced to the loaded state.
/// * `state` - Application state owning signal handles and settings.
/// * `data` - Loaded artist data from [`fetch_artist_detail`].
pub fn apply_artist_detail(
    name_label: &Label,
    album_count_label: &Label,
    albums_container: &Box,
    collapse_button: &ToggleButton,
    state: &Arc<AppState>,
    data: ArtistDetailData,
) {
    name_label.set_label(&data.name);
    album_count_label.set_label(&format!("{} albums", data.album_count));

    let mut tracks_by_album = data.tracks_by_album;
    let mut track_lists: Vec<ListBox> = Vec::new();
    let expanded = !state.storage.get_artist_albums_collapsed();
    for album in &data.albums {
        let fi = data
            .format_info_map
            .get(&album.id)
            .cloned()
            .unwrap_or_default();
        let tracks = tracks_by_album.remove(&album.id).unwrap_or_default();
        let (section, listbox, _) = build_album_section(state, album, &fi, tracks, expanded);
        track_lists.push(listbox);
        albums_container.append(&section);
    }
    set_collapse_all_state(collapse_button, expanded);

    for (i, tb) in track_lists.iter().enumerate() {
        let others: Vec<ListBox> = track_lists
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, lb)| lb.clone())
            .collect();
        state
            .handles
            .lock()
            .retain_signal(tb.connect_row_selected(move |_, row| clear_other_lists(row, &others)));
    }
}

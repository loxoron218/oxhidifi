//! Artist detail page with album groupings and track listings.

use std::{collections::HashMap, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        glib::{idle_add_local, prelude::Cast, spawn_future_local},
        gtk::{
            Align::Start,
            Box as GtkBox, Button,
            ContentFit::Cover,
            GestureClick, Label, ListBox, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            Picture, Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
            prelude::{AccessibleExtManual, BoxExt, ButtonExt, GestureSingleExt, WidgetExt},
        },
    },
    tokio::join,
    tracing::{info, warn},
};

use crate::{
    app::runtime::{
        AppState,
        NavigationEvent::{self, AlbumDetail},
    },
    storage::{
        Storage,
        catalog::{Album, Track},
        formats::FormatInfo,
    },
    ui::{
        detail::{
            cover_art::decode_cover_into_picture,
            page::{build_detail_wrapper, build_scroll_content},
            tracklist::fill_track_list_batch,
        },
        gallery::play_action::play_artist,
    },
};

/// Data loaded from storage for the artist detail page.
///
/// Kept free of widget handles so the fetch future stays `Send`; the widgets
/// are populated separately via [`apply_artist_detail`].
struct ArtistDetailData {
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

/// Build the artist detail page widget.
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

    let play_all_button = Button::builder()
        .label("Play All")
        .icon_name("media-playback-start-symbolic")
        .css_classes(["suggested-action", "pill"])
        .halign(Start)
        .tooltip_text("Play all albums by this artist")
        .can_focus(true)
        .build();
    play_all_button.update_property(&[PropertyLabel("Play all albums by this artist")]);
    let play_state = Arc::clone(state);
    play_all_button.connect_clicked(move |_| {
        let state = Arc::clone(&play_state);
        spawn_future_local(async move {
            play_artist(&state, artist_id).await;
        });
    });
    content.append(&play_all_button);

    let albums_container = GtkBox::builder().orientation(Vertical).spacing(18).build();
    content.append(&albums_container);

    scroll.set_child(Some(&content));
    wrapper.append(&scroll);

    let sc = Arc::clone(state);
    spawn_future_local(async move {
        if let Some(data) = fetch_artist_detail(&sc, artist_id).await {
            apply_artist_detail(
                &name_label,
                &album_count_label,
                &albums_container,
                &sc,
                data,
            );
        }
    });

    wrapper.upcast()
}

/// Load artist detail data from storage.
///
/// Fetches the artist record, albums, format info, and tracks.  Widget updates
/// are applied separately via [`apply_artist_detail`], keeping this future
/// `Send`.
async fn fetch_artist_detail(state: &Arc<AppState>, artist_id: i64) -> Option<ArtistDetailData> {
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
fn apply_artist_detail(
    name_label: &Label,
    album_count_label: &Label,
    albums_container: &GtkBox,
    state: &Arc<AppState>,
    data: ArtistDetailData,
) {
    name_label.set_label(&data.name);
    album_count_label.set_label(&format!("{} albums", data.album_count));

    let mut tracks_by_album = data.tracks_by_album;
    let mut track_lists: Vec<ListBox> = Vec::new();
    for album in &data.albums {
        let fi = data
            .format_info_map
            .get(&album.id)
            .cloned()
            .unwrap_or_default();
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

/// Build a section for a single album in the artist detail page.
///
/// Creates a header with thumbnail, title, metadata and a track list. The
/// header is clickable — clicking it navigates to the album's detail page per
/// US5/AC4.
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
        .tooltip_text(format!("Open {}", album.title))
        .can_focus(true)
        .build();
    album_header.update_property(&[PropertyLabel(&format!("Open album {}", album.title))]);

    let album_id = album.id;
    let nav_state = Arc::clone(state);
    let gesture = GestureClick::new();
    gesture.set_button(1);
    gesture.connect_released(move |_, _, _, _| {
        let ns = Arc::clone(&nav_state);
        spawn_future_local(async move {
            ns.send_navigation_event(AlbumDetail(album_id)).await;
        });
    });
    album_header.add_controller(gesture);

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

        decode_cover_into_picture(state, album.id, art_path.clone(), 60, &thumb);
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
        .map(|(i, t)| (t, i.saturating_add(1)))
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
    use std::slice::from_ref;

    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, ListBox, ListBoxRow, test},
    };

    use crate::ui::detail::artist_page::clear_other_lists;

    #[test]
    fn clear_other_lists_keeps_active_selection() -> Result<()> {
        let active = ListBox::new();
        let other = ListBox::new();
        let active_row = ListBoxRow::new();
        let other_row = ListBoxRow::new();
        active.append(&active_row);
        other.append(&other_row);
        active.select_row(Some(&active_row));
        other.select_row(Some(&other_row));
        ensure!(active.selected_row().is_some());
        ensure!(other.selected_row().is_some());

        clear_other_lists(Some(&active_row), from_ref(&other));

        ensure!(active.selected_row().is_some());
        ensure!(other.selected_row().is_none());
        Ok(())
    }

    #[test]
    fn clear_other_lists_none_row_is_noop() -> Result<()> {
        let other = ListBox::new();
        let other_row = ListBoxRow::new();
        other.append(&other_row);
        other.select_row(Some(&other_row));

        clear_other_lists(None, from_ref(&other));

        ensure!(other.selected_row().is_some());
        Ok(())
    }
}

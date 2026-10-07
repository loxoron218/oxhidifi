//! Per-album section for the artist detail page.
//!
//! Builds the clickable album header (thumbnail, title, metadata) with its
//! track list, split from `artist_page` to keep each module under the
//! file-size limit.

use std::sync::Arc;

use libadwaita::{
    glib::{idle_add_local, spawn_future_local},
    gtk::{
        Align::Start,
        Box,
        ContentFit::Cover,
        GestureClick, Label, ListBox,
        Orientation::{Horizontal, Vertical},
        Picture,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExtManual, BoxExt, GestureSingleExt, WidgetExt},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::AlbumDetail},
    storage::{
        catalog::{Album, Track},
        formats::FormatInfo,
    },
    ui::detail::{cover_art::decode_cover_into_picture, tracklist::fill_track_list_batch},
};

/// Build a section for a single album in the artist detail page.
///
/// Creates a header with thumbnail, title, metadata and a track list. The
/// header is clickable — clicking it navigates to the album's detail page per
/// US5/AC4.
///
/// Returns the section widget and the track list box for selection management.
/// Cover art is loaded asynchronously off the main thread.
///
/// # Arguments
///
/// * `state` - Application state.
/// * `album` - Album to display.
/// * `format_info` - Precomputed format summary for the album.
/// * `tracks` - Tracks belonging to the album, in display order.
///
/// # Returns
///
/// * `(Box, ListBox)` - Section widget and its track list box.
pub fn build_album_section(
    state: &Arc<AppState>,
    album: &Album,
    format_info: &FormatInfo,
    tracks: Vec<Track>,
) -> (Box, ListBox) {
    let section = Box::builder().orientation(Vertical).spacing(6).build();

    let album_header = Box::builder()
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
    state
        .handles
        .lock()
        .retain_signal(gesture.connect_released(move |_, _, _, _| {
            let ns = Arc::clone(&nav_state);
            nav_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    ns.send_navigation_event(AlbumDetail(album_id)).await;
                }));
        }));
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

    let info_box = Box::builder()
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
    let state_cb = Arc::clone(state);
    state.handles.lock().retain_source(idle_add_local(move || {
        fill_track_list_batch(&mut remaining_tracks, &tl, &state_cb)
    }));

    section.append(&track_list);
    (section, track_list)
}

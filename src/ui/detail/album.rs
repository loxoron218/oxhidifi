//! Album detail page with artwork, metadata, and track listing.

use std::{sync::Arc, thread::spawn};

use {
    async_channel::{Sender, bounded},
    libadwaita::{
        gdk::MemoryTexture,
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::Start,
            Box,
            ContentFit::Cover,
            Label, ListBox,
            Orientation::Horizontal,
            Picture, ScrolledWindow, Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End as EllipsizeEnd,
            prelude::{AccessibleExtManual, BoxExt, WidgetExt},
        },
    },
    tracing::{error, info},
};

use crate::{
    app::{AppState, NavigationEvent},
    storage::Storage,
    ui::{
        decode_cover_at_size,
        detail::common::{build_detail_wrapper, build_scroll_content, build_track_row},
    },
};

/// Size of the album cover artwork on the detail page in pixels.
const DETAIL_COVER_SIZE: i32 = 320;

/// Owned widgets for the album detail content area.
struct AlbumDetailContent {
    /// The scroll window wrapping content.
    scroll: ScrolledWindow,
    /// Album artwork display.
    artwork: Picture,
    /// Album title label.
    title_label: Label,
    /// Artist name label.
    artist_label: Label,
    /// Release year label.
    year_label: Label,
    /// Genre label.
    genre_label: Label,
    /// Format summary label.
    format_label: Label,
    /// Track listing container.
    track_list: ListBox,
}

/// Widget references for the album detail page.
struct AlbumDetailWidgets<'a> {
    /// Album artwork display.
    artwork: &'a Picture,
    /// Album title label.
    title_label: &'a Label,
    /// Artist name label.
    artist_label: &'a Label,
    /// Release year label.
    year_label: &'a Label,
    /// Genre label.
    genre_label: &'a Label,
    /// Format summary label (sample rate, bit depth, etc.).
    format_label: &'a Label,
    /// Track listing container.
    track_list: &'a ListBox,
}

/// Decode cover art on a background thread and send through the channel.
///
/// Runs inside a `spawn` closure so the decode does not block the main thread.
fn decode_and_send_cover(tx: &Sender<Option<MemoryTexture>>, path: &str) {
    let texture = decode_cover_at_size(path, DETAIL_COVER_SIZE);
    if let Err(e) = tx.try_send(texture) {
        error!(error = %e, "Failed to send decoded cover art");
    }
}

/// Build the scrollable content area with album widgets.
fn build_album_content() -> AlbumDetailContent {
    let (scroll, content) = build_scroll_content();

    let artwork = Picture::builder()
        .content_fit(Cover)
        .can_shrink(true)
        .css_classes(["album-cover"])
        .build();
    artwork.update_property(&[PropertyLabel("Album artwork")]);

    let artwork_wrapper = Box::builder()
        .orientation(Horizontal)
        .width_request(DETAIL_COVER_SIZE)
        .height_request(DETAIL_COVER_SIZE)
        .halign(Start)
        .build();
    artwork_wrapper.append(&artwork);
    content.append(&artwork_wrapper);

    let title_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(EllipsizeEnd)
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel("Album title")]);
    content.append(&title_label);

    let artist_label = Label::builder()
        .css_classes(["title-4", "accent"])
        .ellipsize(EllipsizeEnd)
        .halign(Start)
        .build();
    artist_label.update_property(&[PropertyLabel("Artist name")]);
    content.append(&artist_label);

    let meta_box = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .halign(Start)
        .build();

    let year_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    year_label.update_property(&[PropertyLabel("Release year")]);
    meta_box.append(&year_label);

    let genre_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    genre_label.update_property(&[PropertyLabel("Genre")]);
    meta_box.append(&genre_label);

    let format_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    format_label.update_property(&[PropertyLabel("Audio format")]);
    meta_box.append(&format_label);

    content.append(&meta_box);

    let tracks_header = Label::builder()
        .label("Tracks")
        .css_classes(["title-4", "heading"])
        .halign(Start)
        .build();
    tracks_header.update_property(&[PropertyLabel("Track list")]);
    content.append(&tracks_header);

    let track_list = ListBox::builder().css_classes(["boxed-list"]).build();
    content.append(&track_list);

    scroll.set_child(Some(&content));

    AlbumDetailContent {
        scroll,
        artwork,
        title_label,
        artist_label,
        year_label,
        genre_label,
        format_label,
        track_list,
    }
}

/// Build the album detail page widget.
#[must_use]
pub fn build_album_detail(
    state: &Arc<AppState>,
    album_id: i64,
    nav_tx: Sender<NavigationEvent>,
) -> Widget {
    let wrapper = build_detail_wrapper(&nav_tx, "Album");

    let content = build_album_content();
    wrapper.append(&content.scroll);

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
                format_label: &content.format_label,
                track_list: &content.track_list,
            },
            nav_tx,
        )
        .await;
    });

    wrapper.upcast()
}

/// Load album data from storage and populate the detail UI elements.
async fn populate_album_detail(
    state: &Arc<AppState>,
    album_id: i64,
    widgets: AlbumDetailWidgets<'_>,
    nav_tx: Sender<NavigationEvent>,
) {
    let album = match state.storage.get_album(album_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(album_id, "Album not found");
            return;
        }
        Err(e) => {
            info!(error = %e, album_id, "Failed to load album");
            return;
        }
    };

    if let Some(path) = &album.artwork_path {
        let path = path.clone();
        let (tx, rx) = bounded(1);
        spawn(move || decode_and_send_cover(&tx, &path));
        if let Ok(Some(texture)) = rx.recv().await {
            widgets.artwork.set_paintable(Some(&texture));
        }
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

    widgets.format_label.set_label(&album.format_summary);

    let tracks = match state.storage.get_tracks_by_album(album_id).await {
        Ok(t) => t,
        Err(e) => {
            info!(error = %e, album_id, "Failed to load album tracks");
            return;
        }
    };

    for (i, track) in tracks.iter().enumerate() {
        let row = build_track_row(state, track, i + 1, &nav_tx);
        widgets.track_list.append(&row);
    }
}

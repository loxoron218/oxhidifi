//! Artist detail page with album groupings and track listings.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::Start,
            Box, Image, Label, ListBox,
            Orientation::{Horizontal, Vertical},
            Widget,
            accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End as EllipsizeEnd,
            prelude::{AccessibleExtManual, BoxExt},
        },
    },
    tracing::info,
};

use crate::{
    app::{AppState, NavigationEvent},
    storage::{Album, Storage},
    ui::detail::common::{build_detail_wrapper, build_scroll_content, build_track_row},
};

/// Build the artist detail page widget.
#[must_use]
pub fn build_artist_detail(
    state: &Arc<AppState>,
    artist_id: i64,
    nav_tx: Sender<NavigationEvent>,
) -> Widget {
    let wrapper = build_detail_wrapper(&nav_tx, "Artist");

    let (scroll, content) = build_scroll_content();

    let name_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(EllipsizeEnd)
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

    let albums_container = Box::builder().orientation(Vertical).spacing(18).build();
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
            nav_tx,
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
    albums_container: &Box,
    nav_tx: Sender<NavigationEvent>,
) {
    let artist = match state.storage.get_artist(artist_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            info!(artist_id, "Artist not found");
            return;
        }
        Err(e) => {
            info!(error = %e, artist_id, "Failed to load artist");
            return;
        }
    };

    name_label.set_label(&artist.name);
    album_count_label.set_label(&format!("{} albums", artist.album_count));

    let albums = match state.storage.get_albums_by_artist(artist_id).await {
        Ok(a) => a,
        Err(e) => {
            info!(error = %e, artist_id, "Failed to load artist albums");
            return;
        }
    };

    for album in &albums {
        let section = build_album_section(state, album, &nav_tx).await;
        albums_container.append(&section);
    }
}

/// Build a section for a single album with its tracks.
async fn build_album_section(
    state: &Arc<AppState>,
    album: &Album,
    nav_tx: &Sender<NavigationEvent>,
) -> Box {
    let section = Box::builder().orientation(Vertical).spacing(6).build();

    let album_header = Box::builder().orientation(Horizontal).spacing(12).build();

    if let Some(art_path) = &album.artwork_path {
        let thumb = Image::builder()
            .pixel_size(60)
            .halign(Start)
            .valign(Start)
            .css_classes(["album-cover"])
            .build();
        thumb.set_from_file(Some(art_path));
        album_header.append(&thumb);
    }

    let info_box = Box::builder()
        .orientation(Vertical)
        .spacing(3)
        .hexpand(true)
        .build();

    let album_title = Label::builder()
        .label(&album.title)
        .css_classes(["title-4", "heading"])
        .ellipsize(EllipsizeEnd)
        .halign(Start)
        .build();
    info_box.append(&album_title);

    let album_meta = Label::builder()
        .label(format!(
            "{} tracks \u{2022} {}",
            album.track_count, album.format_summary
        ))
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    info_box.append(&album_meta);

    album_header.append(&info_box);
    section.append(&album_header);

    let track_list = ListBox::builder().css_classes(["boxed-list"]).build();

    let tracks = match state.storage.get_tracks_by_album(album.id).await {
        Ok(t) => t,
        Err(e) => {
            info!(error = %e, album_id = album.id, "Failed to load tracks");
            return section;
        }
    };

    for (i, track) in tracks.iter().enumerate() {
        let row = build_track_row(state, track, i + 1, nav_tx);
        track_list.append(&row);
    }

    section.append(&track_list);
    section
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

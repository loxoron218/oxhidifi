//! Artist detail page with album groupings and track listings.

use std::{collections::HashMap, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{
        glib::spawn_future_local,
        gtk::{
            Align::Start, Box, Frame, Label, ListBox, ListBoxRow, Orientation::Vertical,
            ToggleButton, Widget, accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, ToggleButtonExt, WidgetExt},
    },
    tokio::join,
    tracing::{error, info, warn},
};

use crate::{
    app::runtime::{AppState, NavigationEvent},
    playback::transport::PlaybackTransport,
    storage::{
        Storage,
        catalog::{Album, Track},
        formats::FormatInfo,
    },
    ui::{
        detail::{
            artist_actions::{
                build_artist_actions, refresh_collapse_all_visual, set_collapse_all_state,
            },
            artist_album_section::{build_album_section, persist_collapse_preference},
            body_compose::DETAIL_COVER_SIZE,
            page::{build_detail_wrapper, build_scroll_content},
        },
        gallery::{
            avatar::build_artist_avatar,
            play_action::{play_artist, play_artist_shuffled},
        },
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

    let artist_image = build_artist_avatar(DETAIL_COVER_SIZE);
    let artist_frame = Frame::builder()
        .child(&artist_image)
        .width_request(DETAIL_COVER_SIZE)
        .height_request(DETAIL_COVER_SIZE)
        .halign(Start)
        .css_classes(["card"])
        .build();
    artist_frame.update_property(&[PropertyLabel("Artist image")]);
    content.append(&artist_frame);

    let name_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(End)
        .hexpand(true)
        .halign(Start)
        .build();
    name_label.update_property(&[PropertyLabel("Artist name")]);
    content.append(&name_label);

    let album_count_label = Label::builder()
        .css_classes(["dim-label", "body"])
        .hexpand(true)
        .halign(Start)
        .build();
    album_count_label.update_property(&[PropertyLabel("Album count")]);
    content.append(&album_count_label);

    let albums_expanded = !state.storage.get_artist_albums_collapsed();
    let (actions, play_button, shuffle_button, collapse_button) =
        build_artist_actions(state.playback.shuffle_enabled(), albums_expanded);
    let shuffle_for_play = shuffle_button.clone();
    let play_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(play_button.connect_clicked(move |_| {
            shuffle_for_play.set_active(false);
            let state_cb = Arc::clone(&play_state);
            play_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    play_artist(&state_cb, artist_id).await;
                }));
        }));
    let shuffle_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(shuffle_button.connect_toggled(move |button| {
            if !button.is_active() {
                disable_shuffle(&shuffle_state);
                return;
            }
            let state_cb = Arc::clone(&shuffle_state);
            shuffle_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    play_artist_shuffled(&state_cb, artist_id).await;
                }));
        }));
    content.append(&actions);

    let albums_container = Box::builder().orientation(Vertical).spacing(18).build();
    content.append(&albums_container);

    scroll.set_child(Some(&content));
    wrapper.append(&scroll);

    wire_collapse_all(state, &collapse_button, &albums_container);

    let sc = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            if let Some(data) = fetch_artist_detail(&sc, artist_id).await {
                apply_artist_detail(
                    &name_label,
                    &album_count_label,
                    &albums_container,
                    &collapse_button,
                    &sc,
                    data,
                );
            }
        }));

    wrapper.upcast()
}

/// Set every loaded album section to the shared expanded state.
///
/// Walks the albums container instead of sharing a widget registry, so no
/// cross-thread state is needed: toggling before data loads simply persists
/// the preference that sections initialize from.
///
/// # Arguments
///
/// * `container` - Albums container holding one section box per album.
/// * `expanded` - Whether sections should be revealed.
fn set_all_sections(container: &Box, expanded: bool) {
    let mut next = container.first_child();
    while let Some(child) = next {
        next = child.next_sibling();
        let Ok(section) = child.downcast::<Box>() else {
            continue;
        };
        let Some(header) = section.first_child() else {
            continue;
        };
        let Ok(header) = header.downcast::<Box>() else {
            continue;
        };
        let Some(toggle) = header.first_child() else {
            continue;
        };
        let Ok(toggle) = toggle.downcast::<ToggleButton>() else {
            continue;
        };
        if toggle.is_active() != expanded {
            toggle.set_active(expanded);
        }
    }
}

/// Wire the collapse-all toggle to every album section.
///
/// Driving each section toggle reuses its own visibility-only handler, so the
/// master fans out the shared state and is the sole writer of the persisted
/// default; the debounced saver coalesces the burst.
///
/// # Arguments
///
/// * `state` - Application state owning signal handles and settings.
/// * `master` - Collapse-all toggle in the header actions.
/// * `container` - Albums container holding the section boxes.
fn wire_collapse_all(state: &Arc<AppState>, master: &ToggleButton, container: &Box) {
    let sections = container.clone();
    let persist = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(master.connect_toggled(move |button| {
            let expanded = button.is_active();
            refresh_collapse_all_visual(button, expanded);
            info!(expanded, "Artist albums collapse-all toggled",);
            set_all_sections(&sections, expanded);
            persist_collapse_preference(&persist, !expanded);
        }));
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

/// Disable shuffle mode, logging failures.
///
/// Called when the shuffle toggle is switched off or when ordered playback
/// starts via the Play button.
///
/// # Arguments
///
/// * `state` - Application state.
fn disable_shuffle(state: &AppState) {
    if let Err(e) = state.playback.set_shuffle_enabled(false) {
        error!(error = %e, "Failed to disable shuffle mode");
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

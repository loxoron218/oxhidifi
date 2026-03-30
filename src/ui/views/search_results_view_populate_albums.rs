//! Album population for `SearchResultsView`.

use std::{collections::HashMap, rc::Rc};

use {
    libadwaita::{
        glib::MainContext,
        gtk::{FlowBox, Label},
        prelude::WidgetExt,
    },
    tracing::error,
};

use crate::{
    error::numeric_conversion::safe_i32_to_u32,
    library::models::{Album, Artist},
    state::app_state::NavigationState::AlbumDetail,
    ui::{
        components::album_card::AlbumCard,
        formatting::create_format_display,
        views::{detail_playback::play_album, search_results_view::AlbumCardContext},
    },
};

/// Populates the album grid section with pre-filtered albums.
///
/// # Arguments
///
/// * `albums` - List of matching albums (already filtered by query)
/// * `all_artists` - List of all artists for artist name lookup
/// * `albums_header` - Albums section header label
/// * `album_flow_box` - Flow box to populate
/// * `ctx` - Album card context with dependencies
///
/// # Returns
///
/// `true` if any albums were found.
#[must_use]
pub fn populate_albums(
    albums: &[Album],
    all_artists: &[Artist],
    albums_header: &Label,
    album_flow_box: &FlowBox,
    ctx: &AlbumCardContext<'_>,
) -> bool {
    if albums.is_empty() {
        albums_header.set_visible(false);
        album_flow_box.set_visible(false);
        return false;
    }

    albums_header.set_visible(true);
    album_flow_box.set_visible(true);

    let any_selected = ctx
        .playback_deps
        .2
        .as_ref()
        .is_some_and(|state| state.has_selected_albums());

    let mut artist_map: HashMap<_, _> = HashMap::with_capacity(all_artists.len());
    for artist in all_artists {
        artist_map.insert(artist.id, artist);
    }

    for album in albums {
        create_and_add_album_card(album, &artist_map, album_flow_box, ctx, any_selected);
    }

    true
}

/// Creates an album card widget and adds it to the flow box.
///
/// # Arguments
///
/// * `album` - Album to create card for
/// * `artist_map` - `HashMap` for O(1) artist lookup by ID
/// * `album_flow_box` - Flow box to add card to
/// * `ctx` - Album card context with playback dependencies
/// * `any_selected` - Whether any album is currently selected
fn create_and_add_album_card(
    album: &Album,
    artist_map: &HashMap<i64, &Artist>,
    album_flow_box: &FlowBox,
    ctx: &AlbumCardContext<'_>,
    any_selected: bool,
) {
    let artist_name = artist_map.get(&album.artist_id).map_or_else(
        || "Unknown Artist".to_string(),
        |artist| artist.name.clone(),
    );

    let format = create_format_display(album).unwrap_or_default();
    let app_state_clone = ctx.playback_deps.2.cloned();
    let album_for_click = album.clone();
    let album_for_card = album_for_click.clone();

    let album_id = album.id;
    let db_clone = ctx.library_db.cloned();
    let (engine_clone, qm_clone, state_for_play, state_for_selection) = (
        ctx.playback_deps.0.cloned(),
        ctx.playback_deps.1.cloned(),
        ctx.playback_deps.2.cloned(),
        ctx.playback_deps.2.cloned(),
    );

    let is_selected = app_state_clone
        .as_ref()
        .is_some_and(|state| state.is_album_selected(album_id));

    let app_state_for_card = app_state_clone;

    let album_cards_for_toggle = Rc::clone(ctx.album_cards);
    let is_syncing_for_toggle = Rc::clone(ctx.is_syncing_selection);

    let cover_size = ctx.playback_deps.2.map_or(120, |state| {
        safe_i32_to_u32(
            state.zoom_manager.get_grid_cover_dimensions().0,
            180,
            "cover_size",
        )
    });

    match AlbumCard::builder()
        .album(album_for_card)
        .artist_name(artist_name)
        .format(format)
        .show_dr_badge(false)
        .compact(true)
        .cover_size(cover_size)
        .selected(is_selected)
        .on_card_clicked(move || {
            if let Some(state) = &app_state_for_card {
                state.update_navigation(AlbumDetail(album_for_click.clone()));
            }
        })
        .on_play_clicked(move || {
            if let (Some(db), Some(engine), Some(qm), Some(state)) = (
                db_clone.clone(),
                engine_clone.clone(),
                qm_clone.clone(),
                state_for_play.clone(),
            ) {
                MainContext::default().spawn_local(async move {
                    play_album(album_id, Some(db), Some(engine), Some(qm), Some(state)).await;
                });
            }
        })
        .on_selection_toggled(move |selected| {
            if is_syncing_for_toggle.get() {
                return;
            }

            if let Some(state) = &state_for_selection {
                if selected {
                    state.select_album(album_id);
                } else {
                    state.deselect_album(album_id);
                }
                let has_selection = state.has_selected_albums();
                for card in album_cards_for_toggle.borrow().iter() {
                    card.selection_checkbox.set_visible(has_selection);
                    card.selection_checkbox.set_can_target(has_selection);
                }
            }
        })
        .build()
    {
        Ok(card) => {
            if any_selected {
                card.selection_checkbox.set_visible(true);
                card.selection_checkbox.set_can_target(true);
            }
            album_flow_box.insert(&card.widget, -1);
            ctx.album_cards.borrow_mut().push(card);
        }
        Err(e) => {
            error!(error = %e, album_id = album.id, "Failed to create album card");
        }
    }
}

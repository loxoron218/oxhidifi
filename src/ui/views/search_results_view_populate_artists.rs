//! Artist population for `SearchResultsView`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use {
    libadwaita::{
        gtk::{FlowBox, Label},
        prelude::WidgetExt,
    },
    tracing::error,
};

use crate::{
    error::numeric_conversion::safe_i32_to_u32,
    library::models::Artist,
    state::app_state::{AppState, NavigationState::ArtistDetail},
    ui::views::artist_grid::ArtistCard,
};

/// Populates the artist grid section with pre-filtered artists.
///
/// # Arguments
///
/// * `artists` - List of matching artists (already filtered by query)
/// * `artists_header` - Artists section header label
/// * `artist_flow_box` - Flow box to populate
/// * `app_state` - Application state reference
/// * `artist_cards` - Container for artist cards
/// * `is_syncing_selection` - Flag to prevent feedback loops
///
/// # Returns
///
/// `true` if any artists were found.
pub fn populate_artists(
    artists: &[Arc<Artist>],
    artists_header: &Label,
    artist_flow_box: &FlowBox,
    app_state: Option<&Arc<AppState>>,
    artist_cards: &Rc<RefCell<Vec<Rc<ArtistCard>>>>,
    is_syncing_selection: &Rc<Cell<bool>>,
) -> bool {
    if artists.is_empty() {
        artists_header.set_visible(false);
        artist_flow_box.set_visible(false);
        return false;
    }

    artists_header.set_visible(true);
    artist_flow_box.set_visible(true);

    for artist in artists {
        let artist_id = artist.id;
        let app_state_clone = app_state.cloned();
        let app_state_for_selection = app_state.cloned();
        let artist_clone = Arc::clone(artist);

        let is_selected = app_state
            .as_ref()
            .is_some_and(|state| state.is_artist_selected(artist_id));

        let any_selected = app_state
            .as_ref()
            .is_some_and(|state| state.has_selected_artists());

        let artist_cards_for_toggle = Rc::clone(artist_cards);
        let is_syncing_for_toggle = Rc::clone(is_syncing_selection);

        let cover_size = app_state.map_or(120, |state| {
            safe_i32_to_u32(
                state.zoom_manager.get_grid_cover_dimensions().0,
                180,
                "cover_size",
            )
        });

        match ArtistCard::builder()
            .artist((**artist).clone())
            .cover_size(cover_size)
            .selected(is_selected)
            .on_card_clicked(move || {
                if let Some(state) = &app_state_clone {
                    state.update_navigation(ArtistDetail((*artist_clone).clone()));
                }
            })
            .on_selection_toggled(move |selected| {
                if is_syncing_for_toggle.get() {
                    return;
                }

                if let Some(state) = &app_state_for_selection {
                    if selected {
                        state.select_artist(artist_id);
                    } else {
                        state.deselect_artist(artist_id);
                    }
                    let has_selection = state.has_selected_artists();
                    for card in artist_cards_for_toggle.borrow().iter() {
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
                artist_flow_box.insert(&card.widget, -1);
                artist_cards.borrow_mut().push(Rc::new(card));
            }
            Err(e) => {
                error!(error = %e, artist_id = artist.id, "Failed to create artist card");
            }
        }
    }

    true
}

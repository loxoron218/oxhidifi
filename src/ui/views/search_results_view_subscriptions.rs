//! Subscription handlers for `SearchResultsView`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use {
    libadwaita::{
        glib::{JoinHandle, MainContext},
        gtk::FlowBox,
        prelude::{CheckButtonExt, WidgetExt},
    },
    tracing::{error, instrument},
};

use crate::{
    audio::engine::PlaybackState::Playing,
    error::numeric_conversion::safe_i32_to_u32,
    state::{
        app_state::{
            AppState,
            AppStateEvent::{
                CurrentTrackChanged, PlaybackStateChanged, QueueChanged, SelectionChanged,
            },
            LibraryTab::{Albums, Artists},
        },
        zoom_manager::ZoomEvent::GridZoomChanged,
    },
    ui::{components::album_card::AlbumCard, views::artist_grid::ArtistCard},
};

/// Creates a subscription that updates album card play state overlays
/// when the playback state or current track changes.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `album_flow_box` - Album flow box for batching widget updates
/// * `album_cards` - Album cards to update
///
/// # Returns
///
/// A join handle for the subscription task.
#[instrument(skip_all, fields(album_flow_box = ?album_flow_box))]
pub fn create_album_playback_subscription(
    app_state: &Arc<AppState>,
    album_flow_box: &FlowBox,
    album_cards: &Rc<RefCell<Vec<AlbumCard>>>,
) -> JoinHandle<()> {
    let state_clone = Arc::clone(app_state);
    let album_cards_clone = Rc::clone(album_cards);
    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            match event.as_ref() {
                CurrentTrackChanged(_) | PlaybackStateChanged(_) | QueueChanged(_) => {
                    let is_playing = state_clone.get_playback_state() == Playing;
                    let current_album_id = state_clone.get_current_album_id();
                    let mut cards = album_cards_clone.borrow_mut();
                    if let Some(current_id) = current_album_id {
                        for card in cards.iter_mut() {
                            let is_current_album = current_id == card.album_id;
                            card.set_playing(is_current_album && is_playing);
                        }
                    } else {
                        for card in cards.iter_mut() {
                            card.set_playing(false);
                        }
                    }
                }
                _ => {}
            }
        }
    })
}

/// Creates a subscription that syncs selection state between `AppState`
/// and the album/artist cards in search results.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `album_cards` - Album cards for selection sync
/// * `artist_cards` - Artist cards for selection sync
/// * `is_syncing` - Flag to prevent feedback loops
///
/// # Returns
///
/// A join handle for the subscription.
pub fn create_selection_subscription(
    app_state: &Arc<AppState>,
    album_cards: &Rc<RefCell<Vec<AlbumCard>>>,
    artist_cards: &Rc<RefCell<Vec<Rc<ArtistCard>>>>,
    is_syncing: &Rc<Cell<bool>>,
) -> JoinHandle<()> {
    let state_clone = Arc::clone(app_state);
    let album_cards_clone = Rc::clone(album_cards);
    let artist_cards_clone = Rc::clone(artist_cards);
    let is_syncing_clone = Rc::clone(is_syncing);
    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            if let SelectionChanged { tab, selected_ids } = event.as_ref() {
                is_syncing_clone.set(true);
                let has_selection = !selected_ids.is_empty();

                match tab {
                    Albums => {
                        let cards = album_cards_clone.borrow();
                        for card in cards.iter() {
                            let is_selected = selected_ids.contains(&card.album_id);
                            card.selection_checkbox.set_visible(has_selection);
                            card.selection_checkbox.set_can_target(has_selection);
                            card.set_has_selection(has_selection);
                            if card.selection_checkbox.is_active() != is_selected {
                                card.set_selection_state(is_selected);
                            }
                        }
                    }
                    Artists => {
                        let cards = artist_cards_clone.borrow();
                        for card in cards.iter() {
                            let is_selected = selected_ids.contains(&card.artist_id);
                            card.selection_checkbox.set_visible(has_selection);
                            card.selection_checkbox.set_can_target(has_selection);
                            card.set_has_selection(has_selection);
                            if card.selection_checkbox.is_active() != is_selected {
                                card.set_selection_state(is_selected);
                            }
                        }
                    }
                }

                is_syncing_clone.set(false);
            }
        }
    })
}

/// Creates a subscription that updates album and artist card cover sizes
/// when the grid zoom level changes.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `album_flow_box` - Album flow box for queueing redraws
/// * `artist_flow_box` - Artist flow box for queueing redraws
/// * `album_cards` - Album cards for size updates
/// * `artist_cards` - Artist cards for size updates
///
/// # Returns
///
/// A join handle for the subscription.
pub fn create_zoom_subscription(
    app_state: &Arc<AppState>,
    album_flow_box: &FlowBox,
    artist_flow_box: &FlowBox,
    album_cards: &Rc<RefCell<Vec<AlbumCard>>>,
    artist_cards: &Rc<RefCell<Vec<Rc<ArtistCard>>>>,
) -> JoinHandle<()> {
    let state_clone = Arc::clone(app_state);
    let album_flow_box_clone = album_flow_box.clone();
    let artist_flow_box_clone = artist_flow_box.clone();
    let album_cards_clone = Rc::clone(album_cards);
    let artist_cards_clone = Rc::clone(artist_cards);
    MainContext::default().spawn_local(async move {
        let rx = state_clone.zoom_manager.subscribe();
        while let Ok(event) = rx.recv().await {
            if let GridZoomChanged(_) = &*event {
                let (dim, _) = state_clone.zoom_manager.get_grid_cover_dimensions();
                let size_u32 = safe_i32_to_u32(dim, 180, "cover_size");

                let mut cards = album_cards_clone.borrow_mut();
                for card in cards.iter_mut() {
                    card.cover_art.update_dimensions(dim, dim);
                    if let Err(e) = card.update_label_max_width_chars(size_u32) {
                        error!(error = %e, album_id = card.album_id, "Failed to update label max width chars");
                    }
                }
                drop(cards);

                let artist = artist_cards_clone.borrow();
                for card in artist.iter() {
                    if let Err(e) = card.update_cover_size(size_u32) {
                        error!(error = %e, "Failed to update artist card cover size");
                    }
                }
                drop(artist);

                album_flow_box_clone.queue_draw();
                artist_flow_box_clone.queue_draw();
            }
        }
    })
}

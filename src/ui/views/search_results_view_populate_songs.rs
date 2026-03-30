//! Song population for `SearchResultsView`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use libadwaita::{
    gio::ListStore,
    glib::BoxedAnyObject,
    gtk::{Button, ColumnView, Label},
    prelude::{ButtonExt, WidgetExt},
};

use crate::{
    library::models::TrackSearchResult, ui::views::search_results_view::SONG_DISPLAY_LIMIT,
};

/// Populates the song column view with search results.
///
/// # Arguments
///
/// * `tracks` - Track search results to display (takes ownership)
/// * `songs_header` - Songs section header label
/// * `column_view` - Column view for song list
/// * `list_store` - List store to populate
/// * `all_tracks` - Container for all tracks
/// * `see_more_button` - Button for expanding/collapsing
/// * `expanded` - Expanded state cell
///
/// # Returns
///
/// `true` if any tracks were found.
pub fn populate_songs(
    tracks: Vec<TrackSearchResult>,
    songs_header: &Label,
    column_view: &ColumnView,
    list_store: &ListStore,
    all_tracks: &Rc<RefCell<Vec<TrackSearchResult>>>,
    see_more_button: &Button,
    expanded: &Rc<Cell<bool>>,
) -> bool {
    if tracks.is_empty() {
        songs_header.set_visible(false);
        column_view.set_visible(false);
        list_store.remove_all();
        see_more_button.set_visible(false);
        return false;
    }

    songs_header.set_visible(true);
    column_view.set_visible(true);

    *all_tracks.borrow_mut() = tracks;

    expanded.set(false);

    list_store.remove_all();

    let tracks_ref = all_tracks.borrow();
    let items: Vec<BoxedAnyObject> = tracks_ref
        .iter()
        .take(SONG_DISPLAY_LIMIT)
        .map(|track| BoxedAnyObject::new(Arc::new(track.clone())))
        .collect();
    list_store.extend_from_slice(&items);
    drop(tracks_ref);

    let track_count = all_tracks.borrow().len();
    if track_count > SONG_DISPLAY_LIMIT {
        let remaining = track_count - SONG_DISPLAY_LIMIT;
        see_more_button.set_label(&format!("See more ({remaining})"));
        see_more_button.set_visible(true);
    } else {
        see_more_button.set_visible(false);
    }

    true
}

//! Population methods for `SearchResultsView`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use {
    libadwaita::{
        gio::ListStore,
        glib::MainContext,
        gtk::{Button, ColumnView, FlowBox, Label, Widget},
        prelude::{Cast, WidgetExt},
    },
    tokio::join,
    tracing::error,
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    library::{
        database::LibraryDatabase,
        models::{Album, Artist, TrackSearchResult},
    },
    state::app_state::AppState,
    ui::{
        components::{album_card::AlbumCard, search_empty_state::SearchEmptyState},
        views::{
            artist_grid::ArtistCard as ArtistCardType,
            search_results_view::{AlbumCardContext, SearchResultsView},
            search_results_view_populate_albums::populate_albums,
            search_results_view_populate_artists::populate_artists,
            search_results_view_populate_songs::populate_songs,
        },
    },
};

/// Widget references captured for async search operations.
struct SearchWidgetRefs {
    /// Songs section header.
    songs_header: Label,
    /// Albums section header.
    albums_header: Label,
    /// Albums flow box container.
    album_flow_box: FlowBox,
    /// Artists section header.
    artists_header: Label,
    /// Artists flow box container.
    artist_flow_box: FlowBox,
    /// Search empty state widget.
    search_empty_state_widget: Widget,
    /// Search empty state component.
    search_empty_state: SearchEmptyState,
    /// Application state reference.
    app_state: Option<Arc<AppState>>,
    /// Audio engine reference.
    audio_engine: Option<Arc<AudioEngine>>,
    /// Queue manager reference.
    queue_manager: Option<Arc<QueueManager>>,
    /// List store for track results.
    list_store: ListStore,
    /// Column view for songs.
    column_view: ColumnView,
    /// All tracks from search.
    all_tracks: Rc<RefCell<Vec<TrackSearchResult>>>,
    /// See more button.
    see_more_button: Button,
    /// Expanded state cell.
    expanded_cell: Rc<Cell<bool>>,
    /// Album cards container.
    album_cards: Rc<RefCell<Vec<AlbumCard>>>,
    /// Artist cards container.
    artist_cards: Rc<RefCell<Vec<Rc<ArtistCardType>>>>,
    /// Selection sync state.
    is_syncing_selection: Rc<Cell<bool>>,
}

/// Captures widget references from the search results view for async operations.
///
/// # Arguments
///
/// * `this` - Reference to the search results view
///
/// # Returns
///
/// A `SearchWidgetRefs` struct containing cloned widget references
fn get_widget_refs(this: &SearchResultsView) -> SearchWidgetRefs {
    SearchWidgetRefs {
        songs_header: this.songs_header.clone(),
        albums_header: this.albums_header.clone(),
        album_flow_box: this.album_flow_box.clone(),
        artists_header: this.artists_header.clone(),
        artist_flow_box: this.artist_flow_box.clone(),
        search_empty_state_widget: this.search_empty_state.widget().clone().upcast(),
        search_empty_state: this.search_empty_state.clone(),
        app_state: this.app_state.clone(),
        audio_engine: this.audio_engine.clone(),
        queue_manager: this.queue_manager.clone(),
        list_store: this.list_store.clone(),
        column_view: this.column_view.clone(),
        all_tracks: Rc::clone(&this.all_tracks),
        see_more_button: this.see_more_button.clone(),
        expanded_cell: Rc::clone(&this.expanded),
        album_cards: Rc::clone(&this.album_cards),
        artist_cards: Rc::clone(&this.artist_cards),
        is_syncing_selection: Rc::clone(&this.is_syncing_selection),
    }
}

/// Clears all search results from the view.
///
/// # Arguments
///
/// * `this` - Reference to the search results view
fn clear_search_results(this: &SearchResultsView) {
    SearchResultsView::clear_songs(this);
    SearchResultsView::clear_albums(this);
    SearchResultsView::clear_artists(this);
}

/// Checks if the view has no library database.
///
/// # Arguments
///
/// * `this` - Reference to the search results view
///
/// # Returns
///
/// `true` if no library database is available, `false` otherwise
fn has_no_library_db(this: &SearchResultsView) -> bool {
    this.library_db.is_none()
}

/// Performs the asynchronous search operation, populating songs, albums, and artists.
///
/// # Arguments
///
/// * `library_db` - Library database for search queries
/// * `query_owned` - The search query string (owned)
/// * `refs` - Widget references for updating the UI
async fn perform_search(
    library_db: Arc<LibraryDatabase>,
    query_owned: String,
    refs: SearchWidgetRefs,
) {
    let mut has_any_results = false;

    let (tracks_result, search_artists_result, albums_result) = join!(
        library_db.search_tracks(&query_owned),
        library_db.search_artists(&query_owned),
        library_db.search_albums(&query_owned)
    );

    let tracks = match tracks_result {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "Failed to search tracks");
            Vec::new()
        }
    };

    has_any_results |= populate_songs(
        tracks,
        &refs.songs_header,
        &refs.column_view,
        &refs.list_store,
        &refs.all_tracks,
        &refs.see_more_button,
        &refs.expanded_cell,
    );

    let search_artists: Vec<Artist> = match search_artists_result {
        Ok(a) => a,
        Err(e) => {
            error!(error = %e, "Failed to search artists");
            Vec::new()
        }
    };

    let all_albums: Vec<Album> = match albums_result {
        Ok(a) => a,
        Err(e) => {
            error!(error = %e, "Failed to search albums");
            Vec::new()
        }
    };

    let mut album_artist_ids = Vec::with_capacity(all_albums.len());
    album_artist_ids.extend(all_albums.iter().map(|album| album.artist_id));
    let album_artists: Vec<Artist> = search_artists
        .iter()
        .filter(|artist| album_artist_ids.contains(&artist.id))
        .cloned()
        .collect();

    has_any_results |= populate_albums(
        &all_albums,
        &album_artists,
        &refs.albums_header,
        &refs.album_flow_box,
        &AlbumCardContext {
            library_db: Some(&library_db),
            playback_deps: (
                refs.audio_engine.as_ref(),
                refs.queue_manager.as_ref(),
                refs.app_state.as_ref(),
            ),
            album_cards: &refs.album_cards,
            is_syncing_selection: &refs.is_syncing_selection,
        },
    );

    has_any_results |= populate_artists(
        &search_artists,
        &refs.artists_header,
        &refs.artist_flow_box,
        refs.app_state.as_ref(),
        &refs.artist_cards,
        &refs.is_syncing_selection,
    );

    if has_any_results {
        refs.search_empty_state.hide();
        refs.search_empty_state_widget.set_visible(false);
    } else {
        refs.search_empty_state.update_search_query(&query_owned);
        refs.search_empty_state.show();
        refs.search_empty_state_widget.set_visible(true);
    }
}

/// Populates the search results view with matching tracks, albums, and artists.
///
/// # Arguments
///
/// * `this` - `SearchResultsView` reference
/// * `query` - The search query string
pub fn populate(this: &mut SearchResultsView, query: &str) {
    clear_search_results(this);

    if query.is_empty() {
        SearchResultsView::hide_all_sections(this);
        this.search_empty_state.hide();
        return;
    }

    if has_no_library_db(this) {
        SearchResultsView::hide_all_sections(this);
        return;
    }

    let Some(library_db) = this.library_db.as_ref() else {
        SearchResultsView::hide_all_sections(this);
        return;
    };
    let library_db = Arc::clone(library_db);
    let widget_refs = get_widget_refs(this);
    let query_owned = query.to_string();

    MainContext::default().spawn_local(async move {
        perform_search(library_db, query_owned, widget_refs).await;
    });
}

//! Unified search results view with Songs, Albums, and Artists sections.

use std::{
    cell::{Cell, RefCell},
    mem::forget,
    rc::Rc,
    sync::Arc,
};

use libadwaita::{
    gio::ListStore,
    gtk::{Button, ColumnView, FlowBox, Label, Widget},
    prelude::{BoxExt, Cast, WidgetExt},
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    library::{database::LibraryDatabase, models::TrackSearchResult},
    state::app_state::AppState,
    ui::{
        components::{album_card::AlbumCard, search_empty_state::SearchEmptyState},
        views::{
            artist_grid::ArtistCard as ArtistCardType,
            search_results_view_builder::SearchResultsViewBuilder,
            search_results_view_methods::{
                connect_row_activation, create_albums_section, create_artists_section,
                create_main_container, create_songs_section, create_view_state,
                forget_subscription_handles, setup_see_more_button,
            },
            search_results_view_population::populate,
            search_results_view_subscriptions::{
                create_album_playback_subscription, create_selection_subscription,
                create_zoom_subscription,
            },
        },
    },
};

/// Maximum number of song rows displayed before showing a "See more" button.
pub const SONG_DISPLAY_LIMIT: usize = 5;

/// Playback dependencies passed to track population.
pub type PlaybackDeps<'a> = (
    Option<&'a Arc<AudioEngine>>,
    Option<&'a Arc<QueueManager>>,
    Option<&'a Arc<AppState>>,
);

/// Container for album cards.
pub type AlbumCards = Rc<RefCell<Vec<AlbumCard>>>;

/// Container for artist cards.
pub type ArtistCards = Rc<RefCell<Vec<Rc<ArtistCardType>>>>;

/// Sync state for selection.
pub type SyncState = Rc<Cell<bool>>;

/// Container for track search results.
pub type TrackResults = Rc<RefCell<Vec<TrackSearchResult>>>;

/// Context needed for populating album cards with play functionality.
pub struct AlbumCardContext<'a> {
    /// Library database reference.
    pub library_db: Option<&'a Arc<LibraryDatabase>>,
    /// Playback dependencies.
    pub playback_deps: PlaybackDeps<'a>,
    /// Album cards for state sync.
    pub album_cards: &'a Rc<RefCell<Vec<AlbumCard>>>,
    /// Whether we are currently syncing selection from `AppState`.
    pub is_syncing_selection: &'a Rc<Cell<bool>>,
}

/// Unified search results view displaying Songs, Albums, and Artists sections.
pub struct SearchResultsView {
    /// Root widget (`ScrolledWindow` > Box).
    pub widget: Widget,
    /// Songs section header label.
    pub songs_header: Label,
    /// Column view for song results.
    pub column_view: ColumnView,
    /// List store for track search results.
    pub list_store: ListStore,
    /// "See more" / "See less" button.
    pub see_more_button: Button,
    /// All tracks from the last search.
    pub all_tracks: Rc<RefCell<Vec<TrackSearchResult>>>,
    /// Whether all tracks are currently expanded.
    pub expanded: Rc<Cell<bool>>,
    /// Albums section header label.
    pub albums_header: Label,
    /// Albums flow box.
    pub album_flow_box: FlowBox,
    /// Artists section header label.
    pub artists_header: Label,
    /// Artists flow box.
    pub artist_flow_box: FlowBox,
    /// Search empty state component.
    pub search_empty_state: SearchEmptyState,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Library database reference.
    pub library_db: Option<Arc<LibraryDatabase>>,
    /// Audio engine reference.
    pub audio_engine: Option<Arc<AudioEngine>>,
    /// Queue manager reference.
    pub queue_manager: Option<Arc<QueueManager>>,
    /// Album cards for playback state overlay sync and selection.
    pub album_cards: Rc<RefCell<Vec<AlbumCard>>>,
    /// Artist cards for selection support.
    pub artist_cards: Rc<RefCell<Vec<Rc<ArtistCardType>>>>,
    /// Whether we are currently syncing selection from `AppState` (prevents feedback loops).
    pub is_syncing_selection: Rc<Cell<bool>>,
}

impl SearchResultsView {
    /// Creates a new `SearchResultsView` instance.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `library_db` - Library database reference
    /// * `audio_engine` - Audio engine reference
    /// * `queue_manager` - Queue manager reference
    ///
    /// # Returns
    ///
    /// A new `SearchResultsView` instance.
    #[must_use]
    pub fn new(
        app_state: Option<Arc<AppState>>,
        library_db: Option<Arc<LibraryDatabase>>,
        audio_engine: Option<Arc<AudioEngine>>,
        queue_manager: Option<Arc<QueueManager>>,
    ) -> Self {
        let main_container = create_main_container();

        let (
            songs_header,
            column_view,
            see_more_button,
            list_store,
            sort_model,
            no_selection,
            play_button_handle,
        ) = create_songs_section(
            library_db.as_ref(),
            audio_engine.as_ref(),
            queue_manager.as_ref(),
            app_state.as_ref(),
        );

        forget(sort_model);
        forget(no_selection);

        let (albums_header, album_flow_box) = create_albums_section();
        let (artists_header, artist_flow_box) = create_artists_section();

        let search_empty_state = SearchEmptyState::builder().is_album_view(true).build();

        main_container.append(&songs_header);
        main_container.append(&column_view);
        main_container.append(&see_more_button);
        main_container.append(&albums_header);
        main_container.append(&album_flow_box);
        main_container.append(&artists_header);
        main_container.append(&artist_flow_box);
        main_container.append(search_empty_state.widget());

        search_empty_state.hide();

        let scrolled_window = libadwaita::gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&main_container)
            .build();

        let (album_cards, artist_cards, is_syncing_selection, all_tracks, expanded) =
            create_view_state();

        setup_see_more_button(
            &see_more_button,
            &list_store,
            &all_tracks,
            &expanded,
            &scrolled_window,
        );

        let album_cards_clone = Rc::clone(&album_cards);
        let album_flow_box_clone = album_flow_box.clone();
        let playback_subscription_handle = app_state.as_ref().map(|state| {
            create_album_playback_subscription(state, &album_flow_box_clone, &album_cards_clone)
        });

        let selection_subscription_handle = app_state.as_ref().map(|state| {
            create_selection_subscription(state, &album_cards, &artist_cards, &is_syncing_selection)
        });

        let album_flow_box_clone = album_flow_box.clone();
        let artist_flow_box_clone = artist_flow_box.clone();
        let album_cards_zoom = Rc::clone(&album_cards);
        let artist_cards_zoom = Rc::clone(&artist_cards);
        let zoom_subscription_handle = app_state.as_ref().map(|state| {
            create_zoom_subscription(
                state,
                &album_flow_box_clone,
                &artist_flow_box_clone,
                &album_cards_zoom,
                &artist_cards_zoom,
            )
        });

        if let Some(state) = &app_state {
            connect_row_activation(&column_view, state);
        }

        forget_subscription_handles(
            playback_subscription_handle,
            selection_subscription_handle,
            zoom_subscription_handle,
            play_button_handle,
        );

        Self {
            widget: scrolled_window.upcast::<Widget>(),
            songs_header,
            column_view,
            list_store,
            see_more_button,
            all_tracks,
            expanded,
            albums_header,
            album_flow_box,
            artists_header,
            artist_flow_box,
            search_empty_state,
            app_state,
            library_db,
            audio_engine,
            queue_manager,
            album_cards,
            artist_cards,
            is_syncing_selection,
        }
    }

    /// Creates a builder for the `SearchResultsView`.
    ///
    /// # Returns
    ///
    /// A new `SearchResultsViewBuilder` instance.
    #[must_use]
    pub fn builder() -> SearchResultsViewBuilder {
        SearchResultsViewBuilder::default()
    }

    /// Populates the search results view with matching tracks, albums, and artists.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    pub fn populate(&mut self, query: &str) {
        populate(self, query);
    }

    /// Clears all content from the search results view.
    pub fn clear(&mut self) {
        self.clear_songs();
        self.clear_albums();
        self.clear_artists();
        self.hide_all_sections();
        self.search_empty_state.hide();
    }

    /// Clears the songs list store and resets see-more state.
    pub fn clear_songs(&self) {
        self.list_store.remove_all();
        self.all_tracks.borrow_mut().clear();
        self.expanded.set(false);
        self.see_more_button.set_visible(false);
    }

    /// Clears the albums flow box and removes all album cards.
    pub fn clear_albums(&self) {
        self.album_cards.borrow_mut().clear();
        while let Some(child) = self.album_flow_box.first_child() {
            self.album_flow_box.remove(&child);
        }
    }

    /// Clears the artists flow box and removes all artist cards.
    pub fn clear_artists(&self) {
        self.artist_cards.borrow_mut().clear();
        while let Some(child) = self.artist_flow_box.first_child() {
            self.artist_flow_box.remove(&child);
        }
    }

    /// Hides all section headers and content widgets.
    pub fn hide_all_sections(&self) {
        self.songs_header.set_visible(false);
        self.column_view.set_visible(false);
        self.see_more_button.set_visible(false);
        self.albums_header.set_visible(false);
        self.album_flow_box.set_visible(false);
        self.artists_header.set_visible(false);
        self.artist_flow_box.set_visible(false);
    }
}

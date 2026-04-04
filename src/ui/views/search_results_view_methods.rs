//! Helper methods for `SearchResultsView`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use libadwaita::{
    gio::ListStore,
    glib::{BoxedAnyObject, JoinHandle, MainContext, Object},
    gtk::{
        Align::{Center, Fill, Start},
        Box, Button, ColumnView, FlowBox, Label, NoSelection,
        Orientation::Vertical,
        ScrolledWindow,
        SelectionMode::None,
        SortListModel,
    },
    prelude::{AdjustmentExt, ButtonExt, Cast, ListModelExt},
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    library::{database::LibraryDatabase, models::TrackSearchResult},
    state::app_state::{AppState, NavigationState::AlbumDetail},
    ui::views::{
        search_results_view::{
            AlbumCards, ArtistCards, SONG_DISPLAY_LIMIT, SyncState, TrackResults,
        },
        search_song_columns::setup_search_song_columns,
    },
};

/// Subscription handles that keep async subscriptions alive.
pub struct SubscriptionHandles {
    /// Playback subscription handle.
    pub playback: Option<JoinHandle<()>>,
    /// Selection subscription handle.
    pub selection: Option<JoinHandle<()>>,
    /// Zoom subscription handle.
    pub zoom: Option<JoinHandle<()>>,
    /// Play button subscription handle.
    pub play_button: Option<JoinHandle<()>>,
}

impl Drop for SubscriptionHandles {
    fn drop(&mut self) {
        self.abort_all();
    }
}

impl SubscriptionHandles {
    /// Aborts all held subscription handles.
    pub fn abort_all(&self) {
        if let Some(handle) = &self.playback {
            handle.abort();
        }
        if let Some(handle) = &self.selection {
            handle.abort();
        }
        if let Some(handle) = &self.zoom {
            handle.abort();
        }
        if let Some(handle) = &self.play_button {
            handle.abort();
        }
    }
}

/// Collects subscription handles to keep them alive.
///
/// # Arguments
///
/// * `playback` - Playback subscription handle
/// * `selection` - Selection subscription handle
/// * `zoom` - Zoom subscription handle
/// * `play_button` - Play button subscription handle
pub fn collect_subscription_handles(
    playback: Option<JoinHandle<()>>,
    selection: Option<JoinHandle<()>>,
    zoom: Option<JoinHandle<()>>,
    play_button: Option<JoinHandle<()>>,
) -> SubscriptionHandles {
    SubscriptionHandles {
        playback,
        selection,
        zoom,
        play_button,
    }
}

/// Creates the main container box with standard spacing and margins.
///
/// # Returns
///
/// A configured `Box` widget.
#[must_use]
pub fn create_main_container() -> Box {
    Box::builder()
        .orientation(Vertical)
        .spacing(24)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build()
}

/// Creates state containers for the view.
///
/// # Returns
///
/// A tuple containing album cards, artist cards, sync state, track results, and expanded state.
#[must_use]
pub fn create_view_state() -> (
    AlbumCards,
    ArtistCards,
    SyncState,
    TrackResults,
    Rc<Cell<bool>>,
) {
    let album_cards = Rc::new(RefCell::new(Vec::new()));
    let artist_cards: ArtistCards = Rc::new(RefCell::new(Vec::new()));
    let is_syncing_selection = Rc::new(Cell::new(false));
    let all_tracks: TrackResults = Rc::new(RefCell::new(Vec::new()));
    let expanded = Rc::new(Cell::new(false));
    (
        album_cards,
        artist_cards,
        is_syncing_selection,
        all_tracks,
        expanded,
    )
}

/// Creates the songs section with column view and see more button.
///
/// # Arguments
///
/// * `library_db` - Library database reference
/// * `audio_engine` - Audio engine reference
/// * `queue_manager` - Queue manager reference
/// * `app_state` - Application state reference
/// * `search_query` - Shared search query cell for highlighting
/// * `accent_color_hex` - Shared accent color cache cell
///
/// # Returns
///
/// A tuple of (`songs_header`, `column_view`, `see_more_button`, `list_store`, `sort_model`,
/// `no_selection`, `play_button_handle`).
#[must_use]
pub fn create_songs_section(
    library_db: Option<&Arc<LibraryDatabase>>,
    audio_engine: Option<&Arc<AudioEngine>>,
    queue_manager: Option<&Arc<QueueManager>>,
    app_state: Option<&Arc<AppState>>,
    search_query: &Rc<RefCell<String>>,
    accent_color_hex: &Rc<RefCell<Option<String>>>,
) -> (
    Label,
    ColumnView,
    Button,
    ListStore,
    SortListModel,
    NoSelection,
    Option<JoinHandle<()>>,
) {
    let songs_header = Label::builder()
        .label("Songs")
        .halign(Start)
        .css_classes(["title-2"])
        .margin_top(12)
        .build();

    let list_store = ListStore::new::<Object>();
    let sort_model = SortListModel::builder().model(&list_store).build();
    let no_selection = NoSelection::new(Some(sort_model.clone()));

    let mut column_view = ColumnView::builder()
        .model(&no_selection)
        .hexpand(true)
        .vexpand(false)
        .css_classes(["track-list"])
        .build();

    let play_button_handle = setup_search_song_columns(
        &mut column_view,
        library_db,
        audio_engine,
        queue_manager,
        app_state,
        search_query,
        accent_color_hex,
    );

    let sorter = column_view.sorter();
    sort_model.set_sorter(sorter.as_ref());

    let see_more_button = Button::builder()
        .label("See more")
        .css_classes(["flat"])
        .halign(Center)
        .visible(false)
        .build();

    (
        songs_header,
        column_view,
        see_more_button,
        list_store,
        sort_model,
        no_selection,
        play_button_handle,
    )
}

/// Creates the albums section with header and flow box.
///
/// # Returns
///
/// A tuple of (`albums_header`, `album_flow_box`).
#[must_use]
pub fn create_albums_section() -> (Label, FlowBox) {
    let albums_header = Label::builder()
        .label("Albums")
        .halign(Start)
        .css_classes(["title-2"])
        .margin_top(12)
        .build();

    let album_flow_box = create_flow_box();

    (albums_header, album_flow_box)
}

/// Creates the artists section with header and flow box.
///
/// # Returns
///
/// A tuple of (`artists_header`, `artist_flow_box`).
#[must_use]
pub fn create_artists_section() -> (Label, FlowBox) {
    let artists_header = Label::builder()
        .label("Artists")
        .halign(Start)
        .css_classes(["title-2"])
        .margin_top(12)
        .build();

    let artist_flow_box = create_flow_box();

    (artists_header, artist_flow_box)
}

/// Sets up the "see more" button click handler.
///
/// # Arguments
///
/// * `see_more_button` - Reference to the button widget
/// * `list_store` - Reference to the list store
/// * `all_tracks` - Reference to all tracks
/// * `expanded` - Reference to expanded state cell
/// * `scrolled_window` - Scrolled window for scroll position reset
pub fn setup_see_more_button(
    see_more_button: &Button,
    list_store: &ListStore,
    all_tracks: &Rc<RefCell<Vec<TrackSearchResult>>>,
    expanded: &Rc<Cell<bool>>,
    scrolled_window: &ScrolledWindow,
) {
    let list_store_clone = list_store.clone();
    let all_tracks_clone = Rc::clone(all_tracks);
    let expanded_clone = Rc::clone(expanded);
    let btn = see_more_button.clone();
    let scrolled = scrolled_window.clone();
    see_more_button.connect_clicked(move |_| {
        let is_expanded = expanded_clone.get();
        if is_expanded {
            let tracks = all_tracks_clone.borrow();
            list_store_clone.remove_all();
            for track in tracks.iter().take(SONG_DISPLAY_LIMIT) {
                let boxed = BoxedAnyObject::new(Arc::new(track.clone()));
                list_store_clone.append(&boxed);
            }
            let remaining = tracks.len().saturating_sub(SONG_DISPLAY_LIMIT);
            btn.set_label(&format!("See more ({remaining})"));
            expanded_clone.set(false);
            scrolled.vadjustment().set_value(0.0);
        } else {
            let tracks = all_tracks_clone.borrow();
            list_store_clone.remove_all();
            for track in tracks.iter() {
                let boxed = BoxedAnyObject::new(Arc::new(track.clone()));
                list_store_clone.append(&boxed);
            }
            btn.set_label("See less");
            expanded_clone.set(true);
        }
    });
}

/// Creates a configured `FlowBox` for card grids.
///
/// # Returns
///
/// A configured `FlowBox` widget.
#[must_use]
pub fn create_flow_box() -> FlowBox {
    FlowBox::builder()
        .halign(Fill)
        .valign(Start)
        .homogeneous(true)
        .max_children_per_line(100)
        .selection_mode(None)
        .row_spacing(6)
        .column_spacing(6)
        .margin_top(6)
        .hexpand(true)
        .vexpand(false)
        .build()
}

/// Connects row activation on the song column view to navigate to album detail.
///
/// # Arguments
///
/// * `column_view` - Column view to connect activation to
/// * `app_state` - Application state for navigation
pub fn connect_row_activation(column_view: &ColumnView, app_state: &Arc<AppState>) {
    let state_clone = Arc::clone(app_state);
    column_view.connect_activate(move |cv, position| {
        let item = cv
            .model()
            .and_then(|model| model.item(position)?.downcast::<BoxedAnyObject>().ok());

        if let Some(boxed) = item {
            let result = boxed.borrow::<Arc<TrackSearchResult>>();
            let album_id = result.album_id;

            if let Some(album) = state_clone
                .get_library_state()
                .albums
                .iter()
                .find(|a| a.id == album_id)
                .cloned()
            {
                let state_clone2 = Arc::clone(&state_clone);
                MainContext::default().spawn_local(async move {
                    state_clone2.update_navigation(AlbumDetail(album));
                });
            }
        }
    });
}

//! Column view component for albums and artists.
//!
//! This module implements the `ColumnListView` component that displays albums or artists
//! in a column view using the GTK4 model/factory pattern with `gio::ListStore`,
//! `gtk::ColumnView`, and `gtk::NoSelection`.

use std::{cell::Cell, collections::HashSet, rc::Rc, sync::Arc};

use {
    libadwaita::{
        gio::ListStore,
        glib::{BoxedAnyObject, JoinHandle, MainContext, Object},
        gtk::{
            Align::Center,
            BitsetIter, Box, ColumnView, ColumnViewColumn, ColumnViewSorter, FilterListModel,
            Label, MultiSelection,
            Orientation::Vertical,
            SortListModel,
            SortType::{Ascending, Descending},
            Widget,
        },
        prelude::{BoxExt, Cast, ListModelExt, SelectionModelExt, SorterExt},
    },
    tracing::{debug, error},
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    config::settings::{SettingsManager, SortOrder},
    library::{
        database::LibraryDatabase,
        models::{Album, Artist},
    },
    state::app_state::{
        AppState,
        NavigationState::{AlbumDetail, ArtistDetail},
    },
    ui::{
        components::search_empty_state::{SearchEmptyState, SearchEmptyStateConfig},
        views::{
            album_columns::{PlaybackContext, setup_album_columns},
            artist_columns::setup_artist_columns,
            column_view_builder::ColumnListViewBuilder,
            column_view_subscriptions::setup_subscriptions,
            column_view_types::{
                ArtistNameCache, ColumnListViewConfig,
                ColumnListViewType::{self, Albums, Artists},
            },
            column_view_updates::{
                clear_view, filter_view_items, set_albums as update_albums,
                set_artists as update_artists, set_show_dr_badges as update_dr_badges,
                update_artist_cache as cache_artist_names,
            },
        },
    },
    update_visibility_by_count,
};

/// Column view for displaying albums or artists with detailed metadata.
///
/// The `ColumnListView` component displays items in a column layout using
/// GTK4's model/factory pattern, with support for sorting, filtering,
/// and real-time updates.
pub struct ColumnListView {
    /// The underlying GTK widget.
    pub widget: Widget,
    /// The column view widget.
    pub column_view: ColumnView,
    /// The selection model for row selection.
    pub multi_selection: MultiSelection,
    /// The `gio::ListStore` containing BoxedAnyObject-wrapped items.
    pub list_store: ListStore,
    /// The filter model for search filtering.
    pub filter_model: FilterListModel,
    /// The sort model for column header sorting.
    pub sort_model: SortListModel,
    /// The count label showing item count.
    pub count_label: Label,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Settings manager reference for persistence.
    settings_manager: Option<Arc<SettingsManager>>,
    /// Configuration options.
    pub config: ColumnListViewConfig,
    /// Search empty state component.
    pub search_empty_state: SearchEmptyState,
    /// Current albums being displayed.
    pub albums: Vec<Arc<Album>>,
    /// Current artists being displayed.
    pub artists: Vec<Arc<Artist>>,
    /// Artist name cache for album columns.
    artist_name_cache: ArtistNameCache,
    /// Flag to prevent feedback loops during selection sync.
    pub is_syncing_selection: Rc<Cell<bool>>,
    /// Zoom subscription handle for cleanup.
    zoom_subscription_handle: Option<JoinHandle<()>>,
    /// Settings subscription handle for cleanup.
    settings_subscription_handle: Option<JoinHandle<()>>,
    /// Playback state subscription handle for cleanup.
    playback_subscription_handle: Option<JoinHandle<()>>,
    /// Play button column state subscription handle for cleanup.
    play_button_column_subscription_handle: Option<JoinHandle<()>>,
    /// Selection subscription handle for cleanup.
    selection_subscription_handle: Option<JoinHandle<()>>,
}

impl ColumnListView {
    /// Creates a new `ColumnListView` component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    /// * `library_db` - Optional library database reference
    /// * `audio_engine` - Optional audio engine reference
    /// * `queue_manager` - Optional queue manager reference
    /// * `view_type` - Type of items to display
    /// * `show_dr_badges` - Whether to show DR badges
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `ColumnListView` instance.
    #[must_use]
    pub fn new(
        app_state: Option<&Arc<AppState>>,
        library_db: Option<&Arc<LibraryDatabase>>,
        audio_engine: Option<&Arc<AudioEngine>>,
        queue_manager: Option<&Arc<QueueManager>>,
        view_type: &ColumnListViewType,
        show_dr_badges: bool,
        compact: bool,
    ) -> Self {
        let settings_manager = app_state.map(|s| {
            let sm = s.settings_manager.read().clone();
            Arc::new(sm)
        });
        let config = ColumnListViewConfig {
            view_type: view_type.clone(),
            show_dr_badges,
            compact,
        };

        let list_store = ListStore::new::<Object>();
        let filter_model = FilterListModel::builder().model(&list_store).build();

        let sort_model = SortListModel::builder().model(&filter_model).build();

        let multi_selection = MultiSelection::new(Some(sort_model.clone()));

        let column_view = ColumnView::builder()
            .model(&multi_selection)
            .hexpand(true)
            .vexpand(true)
            .build();

        let main_container = Box::builder()
            .orientation(Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        let is_album_view = matches!(config.view_type, Albums);
        let search_empty_state = SearchEmptyState::new(SearchEmptyStateConfig { is_album_view });

        let count_label = Label::builder()
            .label("0 Items")
            .halign(Center)
            .margin_top(12)
            .margin_bottom(6)
            .css_classes(["dim-label"])
            .visible(false)
            .build();

        main_container.append(&count_label.clone().upcast::<Widget>());
        main_container.append(&column_view);
        main_container.append(search_empty_state.widget());
        search_empty_state.hide();

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            column_view,
            multi_selection,
            list_store,
            filter_model,
            sort_model,
            count_label,
            app_state: app_state.cloned(),
            settings_manager,
            config,
            search_empty_state,
            albums: Vec::new(),
            artists: Vec::new(),
            artist_name_cache: ArtistNameCache::new(),
            is_syncing_selection: Rc::new(Cell::new(false)),
            zoom_subscription_handle: None,
            settings_subscription_handle: None,
            playback_subscription_handle: None,
            play_button_column_subscription_handle: None,
            selection_subscription_handle: None,
        };

        view.setup_columns(view_type, library_db, audio_engine, queue_manager);
        view.setup_sorting();
        view.apply_saved_sort_state();

        if let Some(state) = app_state {
            view.setup_subscriptions(state);
            view.connect_row_activation(state);
            view.connect_selection_sync(state);
            view.sync_selection_from_state(state);
        }

        view
    }

    /// Sets up columns based on view type.
    ///
    /// # Arguments
    ///
    /// * `view_type` - Type of items to display
    /// * `library_db` - Optional library database reference
    /// * `audio_engine` - Optional audio engine reference
    /// * `queue_manager` - Optional queue manager reference
    fn setup_columns(
        &mut self,
        view_type: &ColumnListViewType,
        library_db: Option<&Arc<LibraryDatabase>>,
        audio_engine: Option<&Arc<AudioEngine>>,
        queue_manager: Option<&Arc<QueueManager>>,
    ) {
        match view_type {
            Albums => {
                let playback_context = PlaybackContext {
                    library_db,
                    audio_engine,
                    queue_manager,
                    app_state: self.app_state.as_ref(),
                };
                self.play_button_column_subscription_handle = setup_album_columns(
                    &mut self.column_view,
                    &self.multi_selection,
                    &self.artist_name_cache,
                    &playback_context,
                    self.config.show_dr_badges,
                );
            }
            Artists => {
                setup_artist_columns(&mut self.column_view, &self.multi_selection, &self.config);
            }
        }
    }

    /// Sets up sorting by connecting the column view's sorter to the sort model.
    fn setup_sorting(&self) {
        let sorter = self.column_view.sorter();
        self.sort_model.set_sorter(sorter.as_ref());

        if let Some(settings_manager) = &self.settings_manager {
            let Some(base_sorter) = sorter else {
                return;
            };
            let Some(cvs) = base_sorter.downcast_ref::<ColumnViewSorter>() else {
                return;
            };
            let column_view = self.column_view.clone();
            let settings_mgr = Arc::clone(settings_manager);
            let view_type = self.config.view_type.clone();

            cvs.connect_changed(move |_sorter, _change| {
                let base_sorter = column_view.sorter();
                let Some(base_sorter) = base_sorter else {
                    return;
                };
                let Some(sorter) = base_sorter.downcast_ref::<ColumnViewSorter>() else {
                    return;
                };

                if let Some(column) = sorter.primary_sort_column() {
                    let column_name = column.title().unwrap_or_default().to_string();
                    let sort_order = match sorter.primary_sort_order() {
                        Ascending => SortOrder::Ascending,
                        Descending => SortOrder::Descending,
                        _ => return,
                    };

                    let column_name_clone = column_name;
                    if let Err(e) = settings_mgr.update_settings_with(|settings| match view_type {
                        Albums => {
                            settings.albums_sort_column = Some(column_name_clone);
                            settings.albums_sort_order = sort_order;
                        }
                        Artists => {
                            settings.artists_sort_column = Some(column_name_clone);
                            settings.artists_sort_order = sort_order;
                        }
                    }) {
                        error!(error = %e, "Failed to persist sort state");
                    }
                }
            });
        }
    }

    /// Applies the saved sort state from settings.
    fn apply_saved_sort_state(&self) {
        let Some(settings_manager) = &self.settings_manager else {
            return;
        };

        let (sort_column, sort_order) = {
            let settings = settings_manager.get_settings();
            match self.config.view_type {
                Albums => (
                    settings.albums_sort_column.clone(),
                    settings.albums_sort_order,
                ),
                Artists => (
                    settings.artists_sort_column.clone(),
                    settings.artists_sort_order,
                ),
            }
        };

        let Some(column_name) = sort_column else {
            debug!("No saved sort state found");
            return;
        };

        debug!(
            "Applying saved sort state: column={:?}, order={:?}",
            column_name, sort_order
        );

        let columns = self.column_view.columns();
        for i in 0..columns.n_items() {
            if let Some(column) = columns.item(i)
                && let Some(col) = column.downcast_ref::<ColumnViewColumn>()
                && col.title().as_deref() == Some(&column_name)
            {
                let gtk_order = match sort_order {
                    SortOrder::Ascending => Ascending,
                    SortOrder::Descending => Descending,
                };
                self.column_view.sort_by_column(Some(col), gtk_order);
                break;
            }
        }
    }

    /// Sets up subscriptions for reactive updates.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state reference
    fn setup_subscriptions(&mut self, state: &Arc<AppState>) {
        let handles = setup_subscriptions(
            state,
            &self.config,
            &self.multi_selection,
            &self.is_syncing_selection,
        );
        self.zoom_subscription_handle = handles.zoom_handle;
        self.settings_subscription_handle = handles.settings_handle;
        self.playback_subscription_handle = handles.playback_handle;
        self.selection_subscription_handle = handles.selection_handle;
    }

    /// Connects row activation to navigate to detail views.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state reference
    fn connect_row_activation(&self, state: &Arc<AppState>) {
        let state_clone = Arc::clone(state);
        let view_type = self.config.view_type.clone();
        self.column_view
            .connect_activate(move |column_view, position| {
                let item = column_view.model().and_then(|model| {
                    model
                        .item(position)
                        .and_then(|obj| obj.downcast::<BoxedAnyObject>().ok())
                });

                if let Some(boxed) = item {
                    match view_type {
                        Albums => {
                            let album = boxed.borrow::<Arc<Album>>();
                            let album_clone = (**album).clone();
                            let state_clone2 = Arc::clone(&state_clone);
                            MainContext::default().spawn_local(async move {
                                state_clone2.update_navigation(AlbumDetail(album_clone));
                            });
                        }
                        Artists => {
                            let artist = boxed.borrow::<Arc<Artist>>();
                            let artist_clone = (**artist).clone();
                            let state_clone2 = Arc::clone(&state_clone);
                            MainContext::default().spawn_local(async move {
                                state_clone2.update_navigation(ArtistDetail(artist_clone));
                            });
                        }
                    }
                }
            });
    }

    /// Creates a `ColumnListView` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `ColumnListViewBuilder` instance.
    #[must_use]
    pub fn builder() -> ColumnListViewBuilder {
        ColumnListViewBuilder::default()
    }

    /// Updates the count label based on the current view type.
    pub fn update_count_label(&self) {
        match &self.config.view_type {
            Albums => self.update_album_count_label(),
            Artists => self.update_artist_count_label(),
        }
    }

    /// Replaces albums in the list store.
    ///
    /// # Arguments
    ///
    /// * `albums` - New vector of albums to display
    pub fn set_albums(&mut self, albums: Vec<Arc<Album>>) {
        self.albums = update_albums(
            &self.list_store,
            albums,
            &self.config,
            &self.search_empty_state,
        );
        self.update_count_label();
        if let Some(state) = &self.app_state {
            self.sync_selection_from_state(state);
        }
    }

    /// Updates the count label for album view.
    fn update_album_count_label(&self) {
        let album_count = self.albums.len();
        let song_count: i64 = self.albums.iter().map(|a| a.track_count).sum();

        update_visibility_by_count!(album_count, self.count_label);

        let label_text = if song_count > 0 {
            if song_count == 1 {
                format!("{album_count} Album (1 Song)")
            } else {
                format!("{album_count} Albums ({song_count} Songs)")
            }
        } else if album_count == 1 {
            "1 Album".to_string()
        } else {
            format!("{album_count} Albums")
        };

        self.count_label.set_text(&label_text);
    }

    /// Replaces artists in the list store.
    ///
    /// # Arguments
    ///
    /// * `artists` - New vector of artists to display
    pub fn set_artists(&mut self, artists: Vec<Arc<Artist>>) {
        self.artists = update_artists(
            &self.list_store,
            artists,
            &self.config,
            &self.search_empty_state,
        );
        self.update_count_label();
        if let Some(state) = &self.app_state {
            self.sync_selection_from_state(state);
        }
    }

    /// Updates the count label for artist view.
    fn update_artist_count_label(&self) {
        let artist_count = self.artists.len();
        let album_count: i64 = self.artists.iter().map(|a| a.album_count).sum();

        update_visibility_by_count!(artist_count, self.count_label);

        let label_text = if album_count > 0 {
            if album_count == 1 {
                format!("{artist_count} Artist (1 Album)")
            } else {
                format!("{artist_count} Artists ({album_count} Albums)")
            }
        } else if artist_count == 1 {
            "1 Artist".to_string()
        } else {
            format!("{artist_count} Artists")
        };

        self.count_label.set_text(&label_text);
    }

    /// Filters items based on a search query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    pub fn filter_view_items(&mut self, query: &str) {
        filter_view_items(
            query,
            &self.filter_model,
            &self.search_empty_state,
            &self.config,
            &self.albums,
            &self.artists,
        );
    }

    /// Connects selection sync to update `AppState` when selection changes in the UI.
    fn connect_selection_sync(&self, state: &Arc<AppState>) {
        let state_clone = Arc::clone(state);
        let view_type = self.config.view_type.clone();
        let is_syncing = Rc::clone(&self.is_syncing_selection);

        self.multi_selection
            .connect_selection_changed(move |selection_model, _, _| {
                if is_syncing.get() {
                    return;
                }

                let mut selected_ids = HashSet::new();
                let selection = selection_model.selection();
                if let Some((mut iter, mut position)) = BitsetIter::init_first(&selection) {
                    loop {
                        if let Some(obj) = selection_model.item(position)
                            && let Ok(boxed) = obj.downcast::<BoxedAnyObject>()
                        {
                            match view_type {
                                Albums => {
                                    let album = boxed.borrow::<Arc<Album>>();
                                    selected_ids.insert(album.id);
                                }
                                Artists => {
                                    let artist = boxed.borrow::<Arc<Artist>>();
                                    selected_ids.insert(artist.id);
                                }
                            }
                        }

                        if let Some(next_pos) = iter.next() {
                            position = next_pos;
                        } else {
                            break;
                        }
                    }
                }

                match view_type {
                    Albums => state_clone.update_album_selection(selected_ids),
                    Artists => state_clone.update_artist_selection(selected_ids),
                }
            });
    }

    /// Synchronizes the selection state from `AppState` to the UI.
    pub fn sync_selection_from_state(&self, state: &Arc<AppState>) {
        self.is_syncing_selection.set(true);

        let library_state = state.get_library_state();
        let selected_ids = match self.config.view_type {
            Albums => &library_state.selected_album_ids,
            Artists => &library_state.selected_artist_ids,
        };

        let selection_model = &self.multi_selection;
        let n_items = selection_model.n_items();

        // Synchronize selection state without manual signal blocking
        // AppState checks will break potential feedback loops.

        for i in 0..n_items {
            if let Some(obj) = selection_model.item(i)
                && let Ok(boxed) = obj.downcast::<BoxedAnyObject>()
            {
                let id = match self.config.view_type {
                    Albums => boxed.borrow::<Arc<Album>>().id,
                    Artists => boxed.borrow::<Arc<Artist>>().id,
                };

                let should_be_selected = selected_ids.contains(&id);
                if selection_model.is_selected(i) != should_be_selected {
                    if should_be_selected {
                        selection_model.select_item(i, false);
                    } else {
                        selection_model.unselect_item(i);
                    }
                }
            }
        }

        self.is_syncing_selection.set(false);
    }

    /// Clears the view by hiding all items.
    ///
    /// This is used when switching tabs with an active search to prevent
    /// the unfiltered view from appearing during the transition.
    pub fn clear_view(&self) {
        clear_view(&self.filter_model);
    }

    /// Replaces albums in the list store.
    ///
    /// # Arguments
    ///
    /// * `artists` - Artists to cache
    pub fn update_artist_cache(&mut self, artists: &[Arc<Artist>]) {
        cache_artist_names(&self.artist_name_cache, artists);
    }

    /// Updates the DR badge visibility setting.
    ///
    /// # Arguments
    ///
    /// * `show` - Whether to show DR badges
    pub fn set_show_dr_badges(&mut self, show: bool) {
        update_dr_badges(&mut self.config, show);
    }

    /// Cleans up subscription handles.
    pub fn cleanup(&mut self) {
        if let Some(handle) = self.zoom_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.settings_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.playback_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.play_button_column_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.selection_subscription_handle.take() {
            handle.abort();
        }
    }
}

impl Drop for ColumnListView {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::views::column_view::ColumnListViewType::{self, Albums};

    #[test]
    fn column_view_type_default() {
        let view_type = ColumnListViewType::default();
        assert_eq!(view_type, Albums);
    }
}

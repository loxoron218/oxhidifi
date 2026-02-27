//! Column view component for albums and artists.
//!
//! This module implements the `ColumnListView` component that displays albums or artists
//! in a column view using the GTK4 model/factory pattern with `gio::ListStore`,
//! `gtk::ColumnView`, and `gtk::NoSelection`.

use std::sync::Arc;

use libadwaita::{
    gio::ListStore,
    glib::{BoxedAnyObject, JoinHandle, MainContext, Object},
    gtk::{
        Box, ColumnView, FilterListModel, NoSelection, Orientation::Vertical, SortListModel, Widget,
    },
    prelude::{BoxExt, Cast, ListModelExt},
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
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
            album_columns::setup_album_columns,
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
    /// The `gio::ListStore` containing BoxedAnyObject-wrapped items.
    pub list_store: ListStore,
    /// The filter model for search filtering.
    pub filter_model: FilterListModel,
    /// The sort model for column header sorting.
    pub sort_model: SortListModel,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Configuration options.
    pub config: ColumnListViewConfig,
    /// Search empty state component.
    pub search_empty_state: SearchEmptyState,
    /// Current albums being displayed.
    pub albums: Vec<Album>,
    /// Current artists being displayed.
    pub artists: Vec<Artist>,
    /// Artist name cache for album columns.
    artist_name_cache: ArtistNameCache,
    /// Zoom subscription handle for cleanup.
    zoom_subscription_handle: Option<JoinHandle<()>>,
    /// Settings subscription handle for cleanup.
    settings_subscription_handle: Option<JoinHandle<()>>,
    /// Playback state subscription handle for cleanup.
    playback_subscription_handle: Option<JoinHandle<()>>,
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
        let config = ColumnListViewConfig {
            view_type: view_type.clone(),
            show_dr_badges,
            compact,
        };

        let list_store = ListStore::new::<Object>();
        let filter_model = FilterListModel::builder().model(&list_store).build();

        let sort_model = SortListModel::builder().model(&filter_model).build();

        let no_selection = NoSelection::new(Some(sort_model.clone()));

        let column_view = ColumnView::builder()
            .model(&no_selection)
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

        main_container.append(&column_view);
        main_container.append(search_empty_state.widget());
        search_empty_state.hide();

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            column_view,
            list_store,
            filter_model,
            sort_model,
            app_state: app_state.cloned(),
            config,
            search_empty_state,
            albums: Vec::new(),
            artists: Vec::new(),
            artist_name_cache: ArtistNameCache::new(),
            zoom_subscription_handle: None,
            settings_subscription_handle: None,
            playback_subscription_handle: None,
        };

        view.setup_columns(view_type, library_db, audio_engine, queue_manager);

        if let Some(state) = app_state {
            view.setup_subscriptions(state);
            view.connect_row_activation(state);
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
                setup_album_columns(
                    &mut self.column_view,
                    &self.artist_name_cache,
                    library_db,
                    audio_engine,
                    queue_manager,
                    self.app_state.as_ref(),
                    self.config.show_dr_badges,
                );
            }
            Artists => {
                setup_artist_columns(&mut self.column_view, &self.config);
            }
        }
    }

    /// Sets up subscriptions for reactive updates.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state reference
    fn setup_subscriptions(&mut self, state: &Arc<AppState>) {
        let handles = setup_subscriptions(state, &self.config);
        self.zoom_subscription_handle = handles.zoom_handle;
        self.settings_subscription_handle = handles.settings_handle;
        self.playback_subscription_handle = handles.playback_handle;
    }

    /// Connects row activation to navigate to detail views.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state reference
    fn connect_row_activation(&self, state: &Arc<AppState>) {
        let state_clone = state.clone();
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
                            let album = boxed.borrow::<Album>();
                            let album_clone = (*album).clone();
                            let state_clone2 = state_clone.clone();
                            MainContext::default().spawn_local(async move {
                                state_clone2.update_navigation(AlbumDetail(album_clone));
                            });
                        }
                        Artists => {
                            let artist = boxed.borrow::<Artist>();
                            let artist_clone = (*artist).clone();
                            let state_clone2 = state_clone.clone();
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

    /// Replaces albums in the list store.
    ///
    /// # Arguments
    ///
    /// * `albums` - New vector of albums to display
    pub fn set_albums(&mut self, albums: Vec<Album>) {
        self.albums = update_albums(
            &self.list_store,
            albums,
            &self.config,
            &self.search_empty_state,
        );
    }

    /// Replaces artists in the list store.
    ///
    /// # Arguments
    ///
    /// * `artists` - New vector of artists to display
    pub fn set_artists(&mut self, artists: Vec<Artist>) {
        self.artists = update_artists(
            &self.list_store,
            artists,
            &self.config,
            &self.search_empty_state,
        );
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
    pub fn update_artist_cache(&mut self, artists: &[Artist]) {
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
    fn test_column_view_type_default() {
        let view_type = ColumnListViewType::default();
        assert_eq!(view_type, Albums);
    }
}

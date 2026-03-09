//! Column view update methods for managing displayed items.

use std::sync::Arc;

use libadwaita::{
    gio::ListStore,
    glib::{BoxedAnyObject, Object},
    gtk::{CustomFilter, FilterListModel},
    prelude::{Cast, ListModelExt},
};

use crate::{
    library::models::{Album, Artist},
    ui::{
        components::search_empty_state::SearchEmptyState,
        views::column_view_types::{
            ArtistNameCache, ColumnListViewConfig,
            ColumnListViewType::{Albums, Artists},
        },
    },
};

/// Replaces albums in the list store.
///
/// # Arguments
///
/// * `list_store` - The list store to update
/// * `albums` - New vector of albums to display
/// * `config` - Configuration options
/// * `search_empty_state` - Search empty state widget
#[must_use]
pub fn set_albums(
    list_store: &ListStore,
    albums: Vec<Arc<Album>>,
    config: &ColumnListViewConfig,
    search_empty_state: &SearchEmptyState,
) -> Vec<Arc<Album>> {
    if !matches!(config.view_type, Albums) {
        return albums;
    }

    list_store.remove_all();

    if !albums.is_empty() {
        for album in &albums {
            let boxed = BoxedAnyObject::new(album.clone());
            list_store.append(&boxed);
        }
    }

    search_empty_state.hide();
    albums
}

/// Replaces artists in the list store.
///
/// # Arguments
///
/// * `list_store` - The list store to update
/// * `artists` - New vector of artists to display
/// * `config` - Configuration options
/// * `search_empty_state` - Search empty state widget
#[must_use]
pub fn set_artists(
    list_store: &ListStore,
    artists: Vec<Arc<Artist>>,
    config: &ColumnListViewConfig,
    search_empty_state: &SearchEmptyState,
) -> Vec<Arc<Artist>> {
    if !matches!(config.view_type, Artists) {
        return artists;
    }

    list_store.remove_all();

    if !artists.is_empty() {
        for artist in &artists {
            let boxed = BoxedAnyObject::new(artist.clone());
            list_store.append(&boxed);
        }
    }

    search_empty_state.hide();
    artists
}

/// Filters items based on a search query.
///
/// # Arguments
///
/// * `query` - Search query string
/// * `filter_model` - Filter model to apply filter to
/// * `search_empty_state` - Search empty state widget
/// * `config` - Configuration options
/// * `albums` - Current albums being displayed
/// * `artists` - Current artists being displayed
pub fn filter_view_items(
    query: &str,
    filter_model: &FilterListModel,
    search_empty_state: &SearchEmptyState,
    config: &ColumnListViewConfig,
    albums: &[Arc<Album>],
    artists: &[Arc<Artist>],
) {
    if query.is_empty() {
        filter_model.set_filter(None::<&CustomFilter>);
        search_empty_state.hide();
        return;
    }

    let normalized_query = query.to_lowercase();

    // Check if library is empty to avoid showing search empty state alongside main empty state
    let is_library_empty = match config.view_type {
        Albums => albums.is_empty(),
        Artists => artists.is_empty(),
    };

    if is_library_empty {
        search_empty_state.hide();
        return;
    }

    let filter = match config.view_type {
        Albums => {
            let q = normalized_query;
            CustomFilter::new(move |item: &Object| -> bool {
                if let Some(boxed) = item.downcast_ref::<BoxedAnyObject>() {
                    let album = boxed.borrow::<Arc<Album>>();
                    return album.title.to_lowercase().contains(&q);
                }
                false
            })
        }
        Artists => {
            let q = normalized_query;
            CustomFilter::new(move |item: &Object| -> bool {
                if let Some(boxed) = item.downcast_ref::<BoxedAnyObject>() {
                    let artist = boxed.borrow::<Arc<Artist>>();
                    return artist.name.to_lowercase().contains(&q);
                }
                false
            })
        }
    };

    filter_model.set_filter(Some(&filter));

    if filter_model.n_items() > 0 {
        search_empty_state.hide();
    } else {
        search_empty_state.update_search_query(query);
        search_empty_state.show();
    }
}

/// Clears the view by hiding all items.
///
/// This is used when switching tabs with an active search to prevent
/// the unfiltered view from appearing during the transition.
///
/// # Arguments
///
/// * `filter_model` - Filter model to apply filter to
pub fn clear_view(filter_model: &FilterListModel) {
    let filter = CustomFilter::new(|_: &Object| false);
    filter_model.set_filter(Some(&filter));
}

/// Updates the artist name cache.
///
/// # Arguments
///
/// * `artist_name_cache` - Cache to update
/// * `artists` - Artists to cache
pub fn update_artist_cache(artist_name_cache: &ArtistNameCache, artists: &[Arc<Artist>]) {
    let mut cache = artist_name_cache.borrow_mut();
    cache.clear();

    for artist in artists {
        cache.insert(artist.id, artist.name.clone());
    }
}

/// Updates the DR badge visibility setting.
///
/// # Arguments
///
/// * `config` - Configuration to update
/// * `show` - Whether to show DR badges
pub fn set_show_dr_badges(config: &mut ColumnListViewConfig, show: bool) {
    config.show_dr_badges = show;
}

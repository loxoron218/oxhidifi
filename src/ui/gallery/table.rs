//! Sortable `GtkColumnView` builders for album and artist libraries.
//!
//! Re-exports [`NarrowState`] and column factories from sibling modules.
//! The two builder functions return a fully wired `GtkColumnView` with
//! column-specific factories, sorters, and click‑to‑navigate handling.

use std::{mem::take, sync::Arc};

use {
    libadwaita::{
        gio::{ListModel, ListStore},
        glib::{
            BoxedAnyObject,
            ControlFlow::{Break, Continue},
            idle_add_local, spawn_future_local,
        },
        gtk::{
            ColumnView, ColumnViewColumn, CustomSorter, NoSelection, SortListModel, Widget,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, Cast, ListModelExt},
    },
    parking_lot::Mutex,
};

use crate::{
    app::runtime::{
        AppState, CachedAlbumData, CachedArtistData,
        NavigationEvent::{self, AlbumDetail, ArtistDetail},
    },
    ui::{
        gallery::{
            boxed_data::{AlbumData, ArtistData},
            columns::{
                PendingCovers, build_artist_icon_column, build_cover_column, build_int_column,
                build_string_column, default_int_format, start_cover_batch_decode,
            },
            narrow_flag::NarrowState,
        },
        zoom::list_cover_size,
    },
};

/// Number of items to append to a `ListStore` per idle callback batch.
const STORE_BATCH_SIZE: usize = 50;

/// Create a `ColumnView` with sort model and no selection from a store.
fn setup_column_view(store: ListStore) -> ColumnView {
    let model: ListModel = store.upcast();
    let sort_model = SortListModel::new(Some(model), None::<CustomSorter>);
    let selection = NoSelection::new(Some(sort_model));

    let column_view = ColumnView::builder()
        .model(&selection)
        .hexpand(true)
        .vexpand(true)
        .build();
    column_view.update_property(&[PropertyLabel("Album library")]);
    column_view
}

/// Append up to `STORE_BATCH_SIZE` album items to the store, constructing
/// each item inside the idle callback.
///
/// Building the `BoxedAnyObject` items (title/artist/format string
/// allocations) is the dominant main-thread cost of the column view, so it
/// is spread across idle callbacks like the grid card population instead of
/// being done synchronously for the whole library. Returns `true` when all
/// indices have been consumed. Artwork paths for uncached covers are
/// collected into `covers` for a single decode dispatch once the store is
/// filled.
fn fill_album_store_batch(
    store: &ListStore,
    cached: &CachedAlbumData,
    remaining: &mut Vec<usize>,
    covers: &mut Vec<(i64, String)>,
) -> bool {
    for _ in 0..STORE_BATCH_SIZE {
        let Some(idx) = remaining.pop() else { break };
        let Some(album) = cached.albums.get(idx) else {
            continue;
        };
        let artist_name = cached
            .artist_names
            .get(&album.artist_id)
            .map_or("Unknown Artist", String::as_str);
        let fi = cached
            .format_info
            .get(&album.id)
            .cloned()
            .unwrap_or_default();
        if let Some(path) = &album.artwork_path {
            covers.push((album.id, path.clone()));
        }
        let data = AlbumData {
            id: album.id,
            title: album.title.clone(),
            artist_name: artist_name.to_string(),
            year: album.year.unwrap_or(0),
            format: fi.formats_display(),
            bit_depth: fi.bit_depth_display(),
            sample_rate: fi.sample_rate_display(),
            artwork_path: album.artwork_path.clone().unwrap_or_default(),
        };
        store.append(&BoxedAnyObject::new(data));
    }
    remaining.is_empty()
}

/// Append up to `STORE_BATCH_SIZE` artist items to the store per idle callback.
fn fill_artist_store_batch(
    store: &ListStore,
    cached: &CachedArtistData,
    remaining: &mut Vec<usize>,
) -> bool {
    for _ in 0..STORE_BATCH_SIZE {
        let Some(idx) = remaining.pop() else { break };
        let Some(artist) = cached.artists.get(idx) else {
            continue;
        };
        store.append(&BoxedAnyObject::new(ArtistData {
            id: artist.id,
            name: artist.name.clone(),
            album_count: artist.album_count,
        }));
    }
    remaining.is_empty()
}

/// Build a fully wired `ColumnView` for albums.
///
/// Columns: Cover, Artist Name, Album Name, Format, Bit Depth,
/// Sample Rate, Year.  Format/Bit Depth/Sample Rate bind to
/// `narrow_state` and hide when the window is narrow.
///
/// # Arguments
///
/// * `state` - Application state (for navigation)
/// * `cached` - Shared album data (albums, artist names, format info)
/// * `narrow_state` - Narrow-mode tracker for adaptive column hiding
/// * `indices` - Display order of `cached.albums`
pub fn build_album_column_view(
    state: &Arc<AppState>,
    cached: &CachedAlbumData,
    narrow_state: &NarrowState,
    indices: &[usize],
) -> Widget {
    let store = ListStore::new::<BoxedAnyObject>();

    let column_view = setup_column_view(store.clone());

    let pending_widgets = Arc::<Mutex<PendingCovers>>::default();

    let cover_size = list_cover_size(state.storage.get_list_zoom_level());
    let cover_col = build_cover_column(cover_size, &state.cover_art_cache, &pending_widgets);
    let artist_col =
        build_string_column("Artist Name", |d: &AlbumData| d.artist_name.clone(), true);
    let album_col = build_string_column("Album Name", |d: &AlbumData| d.title.clone(), true);
    let format_col = build_string_column("Format", |d: &AlbumData| d.format.clone(), false);
    let bit_depth_col =
        build_string_column("Bit Depth", |d: &AlbumData| d.bit_depth.clone(), false);
    let sample_rate_col =
        build_string_column("Sample Rate", |d: &AlbumData| d.sample_rate.clone(), false);
    let year_col = build_int_column("Year", |d: &AlbumData| d.year, default_int_format, false);

    column_view.append_column(&cover_col);
    column_view.append_column(&artist_col);
    column_view.append_column(&album_col);
    column_view.append_column(&format_col);
    column_view.append_column(&bit_depth_col);
    column_view.append_column(&sample_rate_col);
    column_view.append_column(&year_col);

    let nav_state = Arc::clone(state);
    column_view.connect_activate(move |cv, position| {
        if let Some(album_id) = id_at_position::<AlbumData>(cv, position, |d| d.id) {
            navigate_to_event(Arc::clone(&nav_state), AlbumDetail(album_id));
        }
    });

    setup_narrow_bindings(
        narrow_state,
        &[&format_col, &bit_depth_col, &sample_rate_col],
    );

    let cached_owned = CachedAlbumData {
        albums: Arc::clone(&cached.albums),
        artist_names: Arc::clone(&cached.artist_names),
        format_info: Arc::clone(&cached.format_info),
    };
    let state_owned = Arc::clone(state);
    let store_fill = store;
    let mut remaining: Vec<usize> = indices.to_vec();
    remaining.reverse();
    let mut covers: Vec<(i64, String)> = Vec::new();
    idle_add_local(move || {
        if fill_album_store_batch(&store_fill, &cached_owned, &mut remaining, &mut covers) {
            dispatch_column_covers(&state_owned, &pending_widgets, cover_size, &mut covers);
            Break
        } else {
            Continue
        }
    });

    column_view.upcast::<Widget>()
}

/// Dispatch decode requests for the covers collected while filling the
/// album column store, skipping albums that already have a cached texture.
fn dispatch_column_covers(
    state: &Arc<AppState>,
    pending_widgets: &Arc<Mutex<PendingCovers>>,
    cover_size: i32,
    covers: &mut Vec<(i64, String)>,
) {
    let uncached: Vec<(i64, String)> = take(covers)
        .into_iter()
        .filter(|(id, _)| !state.cover_art_cache.has_any(*id))
        .collect();
    if !uncached.is_empty() {
        start_cover_batch_decode(
            state,
            uncached,
            cover_size,
            Arc::clone(&state.cover_art_cache),
            Arc::clone(pending_widgets),
        );
    }
}

/// Build a fully wired `ColumnView` for artists.
///
/// Columns: Artist Icon, Artist Name, Number of Albums.
///
/// # Arguments
///
/// * `state` - Application state (for navigation)
/// * `cached` - Shared artist data
/// * `indices` - Display order of `cached.artists`
pub fn build_artist_column_view(
    state: &Arc<AppState>,
    cached: &CachedArtistData,
    indices: &[usize],
) -> Widget {
    let store = ListStore::new::<BoxedAnyObject>();

    let column_view = setup_column_view(store.clone());

    let icon_col = build_artist_icon_column();
    let name_col = build_string_column("Artist Name", |d: &ArtistData| d.name.clone(), true);
    let albums_col = build_int_column(
        "Albums",
        |d: &ArtistData| d.album_count,
        default_int_format,
        false,
    );

    column_view.append_column(&icon_col);
    column_view.append_column(&name_col);
    column_view.append_column(&albums_col);

    let nav_state = Arc::clone(state);
    column_view.connect_activate(move |cv, position| {
        if let Some(artist_id) = id_at_position::<ArtistData>(cv, position, |d| d.id) {
            navigate_to_event(Arc::clone(&nav_state), ArtistDetail(artist_id));
        }
    });

    let cached_owned = CachedArtistData {
        artists: Arc::clone(&cached.artists),
    };
    let store_fill = store;
    let mut remaining: Vec<usize> = indices.to_vec();
    remaining.reverse();
    idle_add_local(move || {
        if fill_artist_store_batch(&store_fill, &cached_owned, &mut remaining) {
            Break
        } else {
            Continue
        }
    });

    column_view.upcast::<Widget>()
}

/// Bind column visibility to narrow mode changes via async watcher.
fn setup_narrow_bindings(narrow_state: &NarrowState, columns: &[&ColumnViewColumn]) {
    let cols: Vec<ColumnViewColumn> = columns.iter().copied().cloned().collect();
    let rx = narrow_state.subscribe();
    set_columns_visibility(&cols, narrow_state.get());
    spawn_future_local(async move {
        while let Ok(narrow) = rx.recv().await {
            set_columns_visibility(&cols, narrow);
        }
    });
}

/// Set visibility of all columns based on narrow mode.
fn set_columns_visibility(cols: &[ColumnViewColumn], narrow: bool) {
    for col in cols {
        col.set_visible(!narrow);
    }
}

/// Spawn a future to send a navigation event.
fn navigate_to_event(state: Arc<AppState>, event: NavigationEvent) {
    spawn_future_local(async move {
        state.send_navigation_event(event).await;
    });
}

/// Extract the id at the given sort‑model position.
fn id_at_position<T: Clone + Send + 'static>(
    cv: &ColumnView,
    position: u32,
    get_id: fn(&T) -> i64,
) -> Option<i64> {
    let selection = cv.model()?;
    let selection = selection.downcast_ref::<NoSelection>()?;
    let model = selection.model()?;
    let sort_model = model.downcast_ref::<SortListModel>()?;
    let item = sort_model.item(position)?;
    let boxed = item.downcast_ref::<BoxedAnyObject>()?;
    Some(get_id(&boxed.borrow::<T>()))
}

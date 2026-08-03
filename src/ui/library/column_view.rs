//! Sortable `GtkColumnView` builders for album and artist libraries.
//!
//! Re-exports [`NarrowState`] and column factories from sibling modules.
//! The two builder functions return a fully wired `GtkColumnView` with
//! column-specific factories, sorters, and click‑to‑navigate handling.

use std::{collections::HashMap, hash::BuildHasher, mem::take, sync::Arc};

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
    app::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail},
    },
    storage::{
        formats::FormatInfo,
        records::{Album, Artist},
    },
    ui::library::{
        columns::{
            PendingCovers, build_artist_icon_column, build_cover_column, build_int_column,
            build_string_column, default_int_format, start_cover_batch_decode,
        },
        models::{AlbumData, ArtistData},
        narrow_state::NarrowState,
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

/// Append up to `STORE_BATCH_SIZE` items from `remaining` to the store.
/// Returns `true` when all items have been consumed.
fn fill_store_batch(remaining: &mut Vec<BoxedAnyObject>, store: &ListStore) -> bool {
    for _ in 0..STORE_BATCH_SIZE {
        let Some(data) = remaining.pop() else { break };
        store.append(&data);
    }
    remaining.is_empty()
}

/// Populate a `ListStore` from `items` in batches via `idle_add_local`.
///
/// Each idle callback appends up to 50 items, keeping the UI responsive
/// during large initial loads. `items` is drained and replaced empty.
fn batched_fill_store(store: &ListStore, items: &mut Vec<BoxedAnyObject>) {
    if items.is_empty() {
        return;
    }
    items.reverse();
    let s = store.clone();
    let mut remaining = take(items);
    idle_add_local(move || {
        if fill_store_batch(&mut remaining, &s) {
            Break
        } else {
            Continue
        }
    });
}

/// Build a fully wired `ColumnView` for albums.
///
/// Columns: Cover, Artist Name, Album Name, Format, Bit Depth,
/// Sample Rate, Year.  Format/Bit Depth/Sample Rate bind to
/// `narrow_state` and hide when the window is narrow.
///
/// # Arguments
///
/// * `state` – Application state (for navigation)
/// * `albums` – Albums to display
/// * `artist_names` – Map of artist id → display name
/// * `narrow_state` – Narrow‑mode tracker for adaptive hiding
/// * `format_info` – Map of album id → distinct format info
pub fn build_album_column_view<S: BuildHasher>(
    state: &Arc<AppState>,
    albums: &[Album],
    artist_names: &HashMap<i64, String, S>,
    narrow_state: &NarrowState,
    format_info: &HashMap<i64, FormatInfo, S>,
) -> Widget {
    let store = ListStore::new::<BoxedAnyObject>();

    let column_view = setup_column_view(store.clone());

    let pending_widgets = Arc::<Mutex<PendingCovers>>::default();

    let cover_col = build_cover_column(&state.cover_art_cache, &pending_widgets);
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

    let mut items: Vec<BoxedAnyObject> = albums
        .iter()
        .map(|album| {
            let artist_name = artist_names
                .get(&album.artist_id)
                .map_or("Unknown Artist", String::as_str);
            let fi = format_info.get(&album.id).cloned().unwrap_or_default();
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
            BoxedAnyObject::new(data)
        })
        .collect();
    batched_fill_store(&store, &mut items);

    let uncached: Vec<(i64, String)> = albums
        .iter()
        .filter_map(|a| a.artwork_path.as_ref().map(|p| (a.id, p.clone())))
        .filter(|(id, _)| state.cover_art_cache.get(*id).is_none())
        .collect();
    if !uncached.is_empty() {
        start_cover_batch_decode(
            uncached,
            Arc::clone(&state.cover_art_cache),
            pending_widgets,
        );
    }

    column_view.upcast::<Widget>()
}

/// Build a fully wired `ColumnView` for artists.
///
/// Columns: Artist Icon, Artist Name, Number of Albums.
pub fn build_artist_column_view(state: &Arc<AppState>, artists: &[Artist]) -> Widget {
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

    let mut items: Vec<BoxedAnyObject> = artists
        .iter()
        .map(|artist| {
            BoxedAnyObject::new(ArtistData {
                id: artist.id,
                name: artist.name.clone(),
                album_count: artist.album_count,
            })
        })
        .collect();
    batched_fill_store(&store, &mut items);

    column_view.upcast::<Widget>()
}

/// Bind column visibility to narrow mode changes via async watcher.
fn setup_narrow_bindings(narrow_state: &NarrowState, columns: &[&ColumnViewColumn]) {
    let cols: Vec<ColumnViewColumn> = columns.iter().copied().cloned().collect();
    let mut rx = narrow_state.subscribe();
    set_columns_visibility(&cols, *rx.borrow());
    spawn_future_local(async move {
        while rx.changed().await.is_ok() {
            set_columns_visibility(&cols, *rx.borrow());
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

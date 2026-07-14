//! Sortable `GtkColumnView` builders for album and artist libraries.
//!
//! Provides `NarrowState` for adaptive column hiding and two builder
//! functions that return a fully wired `GtkColumnView` with
//! column-specific factories, sorters, and click‑to‑navigate handling.

use std::{
    cmp::Ordering::{self, Equal},
    collections::HashMap,
    hash::BuildHasher,
    mem::take,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use {
    async_channel::{TryRecvError::Closed, unbounded},
    libadwaita::{
        gio::{ListModel, ListStore},
        glib::{
            BoxedAnyObject,
            ControlFlow::{Break, Continue},
            Object, WeakRef, idle_add_local, spawn_future_local,
        },
        gtk::{
            Align::Start, ColumnView, ColumnViewColumn, ContentFit::Cover, CustomSorter, Image,
            Label, ListItem, NoSelection, Picture, SignalListItemFactory, SortListModel, Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{Cast, ListItemExt, ListModelExt, ObjectExt},
    },
    parking_lot::Mutex,
    tokio::sync::watch::{Receiver, Sender as TokioSender, channel as TokioChannel},
    tracing::debug,
};

use crate::{
    app::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail},
    },
    storage::{Album, Artist, FormatInfo},
    ui::{
        CoverArtCache, DecodedCover,
        library::models::{AlbumData, ArtistData},
        raw_to_texture,
    },
};

/// Unwrap a `ListItem` and extract a `Ref<T>` from its `BoxedAnyObject`.
macro_rules! with_list_item_data {
    ($item:expr, $ty:ty, $list_item:ident, $data:ident => $body:expr) => {{
        let Some($list_item) = $item.downcast_ref::<ListItem>() else {
            return;
        };
        let Some(item) = $list_item.item() else {
            return;
        };
        let Some(boxed) = item.downcast_ref::<BoxedAnyObject>() else {
            return;
        };
        let $data = boxed.borrow::<$ty>();
        $body
    }};
}

/// Thumbnail size for cover art in the column view.
const COVER_THUMB_SIZE: i32 = 36;

/// Number of items to append to a `ListStore` per idle callback batch.
const STORE_BATCH_SIZE: usize = 50;

/// Tracks whether the window is in narrow‑width mode.
///
/// Created via [`NarrowState::new_shared`] and shared via [`Arc`].
/// Subscribe to changes with [`NarrowState::subscribe`].
pub struct NarrowState {
    /// Whether the window is in narrow mode.
    narrow: AtomicBool,
    /// Channel to notify subscribers of narrow-mode changes.
    tx: TokioSender<bool>,
}

impl NarrowState {
    /// Create a new `NarrowState` wrapped in an [`Arc`].
    #[must_use]
    pub fn new_shared() -> Arc<Self> {
        let (tx, _rx) = TokioChannel(false);
        Arc::new(Self {
            narrow: AtomicBool::new(false),
            tx,
        })
    }

    /// Set the narrow state and notify all subscribers.
    pub fn set(&self, val: bool) {
        self.narrow.store(val, Relaxed);
        if let Err(e) = self.tx.send(val) {
            debug!(error = %e, "No narrow state subscribers");
        }
    }

    /// Return the current narrow state.
    #[must_use]
    pub fn get(&self) -> bool {
        self.narrow.load(Relaxed)
    }

    /// Subscribe to narrow state changes.
    ///
    /// The receiver will immediately yield the current value on first
    /// [`changed`](watch::Receiver::changed) call.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<bool> {
        self.tx.subscribe()
    }
}

/// Map of album ID to pending `Picture` weak references awaiting cover art.
type PendingCovers = HashMap<i64, Vec<WeakRef<Picture>>>;

/// Create a `ColumnView` with sort model and no selection from a store.
fn setup_column_view(store: ListStore) -> ColumnView {
    let model: ListModel = store.upcast();
    let sort_model = SortListModel::new(Some(model), None::<CustomSorter>);
    let selection = NoSelection::new(Some(sort_model));

    ColumnView::builder()
        .model(&selection)
        .hexpand(true)
        .vexpand(true)
        .build()
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
#[must_use]
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
#[must_use]
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

/// Build a cover art column with a 36‑px fixed‑width `Picture`.
///
/// Performs a synchronous cache lookup on bind.  If the texture is not
/// yet cached, the `Picture` widget is registered in `pending_widgets`
/// so a background batch decoder can install the texture later.
///
/// # Arguments
///
/// * `cache` – Shared cover art cache (from [`AppState::cover_art_cache`]).
/// * `pending_widgets` – Map of album ID → weak references to `Picture` widgets that still need
///   their cover art installed.
fn build_cover_column(
    cache: &Arc<CoverArtCache>,
    pending_widgets: &Arc<Mutex<PendingCovers>>,
) -> ColumnViewColumn {
    let factory = SignalListItemFactory::new();

    let cache = Arc::clone(cache);
    let pending = Arc::clone(pending_widgets);

    factory.connect_setup(|_, item: &Object| {
        let picture = Picture::builder()
            .content_fit(Cover)
            .width_request(COVER_THUMB_SIZE)
            .height_request(COVER_THUMB_SIZE)
            .css_classes(["album-cover", "dim-label"])
            .build();
        if let Some(list_item) = item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&picture.upcast::<Widget>()));
        }
    });

    factory.connect_bind(move |_, item: &Object| {
        with_list_item_data!(item, AlbumData, list_item, data => {
            let Some(child) = list_item.child() else {
                return;
            };
            let Some(picture_ref) = child.downcast_ref::<Picture>() else {
                return;
            };

            if data.artwork_path.is_empty() {
                return;
            }

            let album_id = data.id;

            if let Some(texture) = cache.get(album_id) {
                picture_ref.set_paintable(Some(&*texture));
                return;
            }

            pending
                .lock()
                .entry(album_id)
                .or_default()
                .push(picture_ref.downgrade());
        });
    });

    ColumnViewColumn::builder()
        .factory(&factory)
        .fixed_width(COVER_THUMB_SIZE + 12)
        .resizable(false)
        .build()
}

/// Apply a decoded cover to the cache and update any waiting widgets.
fn apply_cover_to_widgets(
    album_id: i64,
    decoded: &DecodedCover,
    cover_cache: &CoverArtCache,
    pending_widgets: &Mutex<PendingCovers>,
) {
    let texture = raw_to_texture(decoded);
    cover_cache.insert(album_id, texture.clone());

    if let Some(waiters) = pending_widgets.lock().remove(&album_id) {
        for weak in waiters {
            weak.upgrade()
                .inspect(|pic| pic.set_paintable(Some(&texture)));
        }
    }
}

/// Send decode requests for a list of album cover paths to the centralized
/// cover decoder.  Results are processed on the main thread via
/// [`idle_add_local`] where they are inserted into the cache and applied
/// to any waiting `Picture` widgets.
fn start_cover_batch_decode(
    albums: Vec<(i64, String)>,
    cover_cache: Arc<CoverArtCache>,
    pending_widgets: Arc<Mutex<PendingCovers>>,
) {
    let (tx, rx) = unbounded::<(i64, DecodedCover)>();

    for (album_id, path) in albums {
        cover_cache.request_decode_to_channel(
            album_id,
            path,
            COVER_THUMB_SIZE,
            tx.clone(),
            "column view",
        );
    }
    drop(tx);

    idle_add_local(move || {
        while let Ok((album_id, decoded)) = rx.try_recv() {
            apply_cover_to_widgets(album_id, &decoded, &cover_cache, &pending_widgets);
        }
        match rx.try_recv() {
            Err(Closed) => Break,
            _ => Continue,
        }
    });
}

/// Build an artist icon column with a 32‑px fixed‑width `Image`.
fn build_artist_icon_column() -> ColumnViewColumn {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, item: &Object| {
        let image = Image::builder()
            .icon_name("avatar-default-symbolic")
            .pixel_size(32)
            .width_request(32)
            .height_request(32)
            .css_classes(["artist-avatar", "dim-label"])
            .build();
        if let Some(list_item) = item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&image.upcast::<Widget>()));
        }
    });

    ColumnViewColumn::builder()
        .factory(&factory)
        .fixed_width(44)
        .resizable(false)
        .build()
}

/// Build a label column with a shared factory, bind, and sorter.
fn build_label_column<T: Clone + Send + 'static>(
    title: &str,
    get_text: impl Fn(&T) -> String + 'static,
    compare: impl Fn(&T, &T) -> Ordering + 'static,
    expand: bool,
) -> ColumnViewColumn {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, item: &Object| {
        let label = Label::builder()
            .ellipsize(End)
            .halign(Start)
            .css_classes(["body"])
            .build();
        if let Some(list_item) = item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&label.upcast::<Widget>()));
        }
    });

    let get_text = Arc::new(get_text);
    let compare = Arc::new(compare);

    let gt = Arc::clone(&get_text);
    factory.connect_bind(move |_, item: &Object| {
        with_list_item_data!(item, T, list_item, data => {
            let Some(child) = list_item.child() else { return };
            let Some(label) = child.downcast_ref::<Label>() else { return };
            label.set_label(&gt(&data));
        });
    });

    let ct = Arc::clone(&compare);
    let sorter = CustomSorter::new(move |a, b| {
        let a_data = a.downcast_ref::<BoxedAnyObject>().map(|o| o.borrow::<T>());
        let b_data = b.downcast_ref::<BoxedAnyObject>().map(|o| o.borrow::<T>());
        match (a_data, b_data) {
            (Some(a), Some(b)) => ct(&*a, &*b).into(),
            _ => Equal.into(),
        }
    });

    ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .sorter(&sorter)
        .resizable(true)
        .expand(expand)
        .build()
}

/// Build a text column that sorts by the given extractor function.
fn build_string_column<T: Clone + Send + 'static>(
    title: &str,
    extract: fn(&T) -> String,
    expand: bool,
) -> ColumnViewColumn {
    build_label_column(
        title,
        extract,
        move |a: &T, b: &T| extract(a).cmp(&extract(b)),
        expand,
    )
}

/// Build an integer column with a format function.
fn build_int_column<T: Clone + Send + 'static>(
    title: &str,
    extract: fn(&T) -> i32,
    format: fn(i32) -> String,
    expand: bool,
) -> ColumnViewColumn {
    build_label_column(
        title,
        move |d: &T| format(extract(d)),
        move |a: &T, b: &T| extract(a).cmp(&extract(b)),
        expand,
    )
}

/// Format an integer for display in a column, returning empty string for 0.
fn default_int_format(n: i32) -> String {
    if n == 0 { String::new() } else { n.to_string() }
}

/// Bind column visibility to narrow mode changes via async watcher.
fn setup_narrow_bindings(narrow_state: &NarrowState, columns: &[&ColumnViewColumn]) {
    let cols: Vec<ColumnViewColumn> = columns.iter().copied().cloned().collect();
    let mut rx = narrow_state.subscribe();
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

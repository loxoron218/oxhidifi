//! Sortable `GtkColumnView` builders for album and artist libraries.
//!
//! Provides `NarrowState` for adaptive column hiding and two builder
//! functions that return a fully wired `GtkColumnView` with
//! column-specific factories, sorters, and click‑to‑navigate handling.

use std::{
    cmp::Ordering::{self, Equal},
    collections::HashMap,
    hash::BuildHasher,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::channel,
    },
};

use {
    libadwaita::{
        gio::{ListModel, ListStore, spawn_blocking},
        glib::{
            BoxedAnyObject,
            ControlFlow::{Break, Continue},
            Object, idle_add_local, spawn_future_local,
        },
        gtk::{
            Align::Start, ColumnView, ColumnViewColumn, ContentFit::Cover, CustomSorter, Image,
            Label, ListItem, NoSelection, Picture, SignalListItemFactory, SortListModel, Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{Cast, ListItemExt, ListModelExt},
    },
    tokio::sync::watch::{Receiver, Sender, channel as TokioChannel},
    tracing::debug,
};

use crate::{
    app::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail},
    },
    storage::{Album, Artist},
    ui::{
        decode_cover_at_size,
        library::models::{AlbumData, ArtistData},
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

/// Tracks whether the window is in narrow‑width mode.
///
/// Created via [`NarrowState::new_shared`] and shared via [`Arc`].
/// Subscribe to changes with [`NarrowState::subscribe`].
pub struct NarrowState {
    /// Whether the window is in narrow mode.
    narrow: AtomicBool,
    /// Channel to notify subscribers of narrow-mode changes.
    tx: Sender<bool>,
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
#[must_use]
pub fn build_album_column_view<S: BuildHasher>(
    state: &Arc<AppState>,
    albums: &[Album],
    artist_names: &HashMap<i64, String, S>,
    narrow_state: &NarrowState,
) -> Widget {
    let store = ListStore::new::<BoxedAnyObject>();
    for album in albums {
        let artist_name = artist_names
            .get(&album.artist_id)
            .map_or("Unknown Artist", String::as_str);
        let data = AlbumData {
            id: album.id,
            title: album.title.clone(),
            artist_name: artist_name.to_string(),
            year: album.year.unwrap_or(0),
            format: album.format.clone(),
            bit_depth: album.bit_depth.unwrap_or(0),
            sample_rate: album.sample_rate.unwrap_or(0),
            artwork_path: album.artwork_path.clone().unwrap_or_default(),
        };
        store.append(&BoxedAnyObject::new(data));
    }

    let column_view = setup_column_view(store);

    let cover_col = build_cover_column();
    let artist_col =
        build_string_column("Artist Name", |d: &AlbumData| d.artist_name.clone(), true);
    let album_col = build_string_column("Album Name", |d: &AlbumData| d.title.clone(), true);
    let format_col = build_string_column("Format", |d: &AlbumData| d.format.clone(), false);
    let bit_depth_col = build_int_column(
        "Bit Depth",
        |d: &AlbumData| d.bit_depth,
        default_int_format,
        false,
    );
    let sample_rate_col = build_int_column(
        "Sample Rate",
        |d: &AlbumData| d.sample_rate,
        sample_rate_format,
        false,
    );
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

    column_view.upcast::<Widget>()
}

/// Build a fully wired `ColumnView` for artists.
///
/// Columns: Artist Icon, Artist Name, Number of Albums.
#[must_use]
pub fn build_artist_column_view(state: &Arc<AppState>, artists: &[Artist]) -> Widget {
    let store = ListStore::new::<BoxedAnyObject>();
    for artist in artists {
        let data = ArtistData {
            id: artist.id,
            name: artist.name.clone(),
            album_count: artist.album_count,
        };
        store.append(&BoxedAnyObject::new(data));
    }

    let column_view = setup_column_view(store);

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

    column_view.upcast::<Widget>()
}

/// Build a cover art column with a 36‑px fixed‑width `Picture`.
fn build_cover_column() -> ColumnViewColumn {
    let factory = SignalListItemFactory::new();

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

    factory.connect_bind(|_, item: &Object| {
        with_list_item_data!(item, AlbumData, list_item, data => {
            let Some(child) = list_item.child() else {
                return;
            };
            let Some(picture) = child.downcast_ref::<Picture>() else {
                return;
            };

            let path = &data.artwork_path;
            if path.is_empty() {
                return;
            }

            let picture_clone = picture.clone();
            let decode_path = path.clone();
            let (tx, rx) = channel();
            spawn_blocking(move || {
                let texture = decode_cover_at_size(&decode_path, COVER_THUMB_SIZE);
                if let Err(e) = tx.send(texture) {
                    debug!(error = %e, "Failed to send decoded cover texture");
                }
            });

            idle_add_local(move || match rx.try_recv() {
                Ok(Some(texture)) => {
                    picture_clone.set_paintable(Some(&texture));
                    Break
                }
                Ok(None) | Err(_) => Continue,
            });
        });
    });

    ColumnViewColumn::builder()
        .factory(&factory)
        .fixed_width(COVER_THUMB_SIZE + 12)
        .resizable(false)
        .build()
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

/// Format sample rate value for display, returning empty string for 0.
fn sample_rate_format(n: i32) -> String {
    if n == 0 {
        String::new()
    } else {
        format_sample_rate_short(n)
    }
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

/// Format sample rate from Hz to a short kHz string (e.g. `44100` → `"44.1"`).
#[must_use]
fn format_sample_rate_short(hz: i32) -> String {
    if hz % 1000 == 0 {
        (hz / 1000).to_string()
    } else {
        format!("{:.1}", f64::from(hz) / 1000.0)
    }
}

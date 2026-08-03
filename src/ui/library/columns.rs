//! Reusable `GtkColumnView` column factories and cover-art decoding.

use std::{
    cmp::Ordering::{self, Equal},
    collections::HashMap,
    sync::Arc,
};

use {
    async_channel::{TryRecvError::Closed, unbounded},
    libadwaita::{
        glib::{
            BoxedAnyObject,
            ControlFlow::{Break, Continue},
            Object, WeakRef, idle_add_local,
        },
        gtk::{
            Align::Start, ColumnViewColumn, ContentFit::Cover, CustomSorter, Image, Label,
            ListItem, Picture, SignalListItemFactory, Widget,
            accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, Cast, ListItemExt, ObjectExt},
    },
    parking_lot::Mutex,
};

use crate::ui::{CoverArtCache, DecodedCover, library::models::AlbumData, raw_to_texture};

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

/// Map of album ID to pending `Picture` weak references awaiting cover art.
pub type PendingCovers = HashMap<i64, Vec<WeakRef<Picture>>>;

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
pub fn build_cover_column(
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
        picture.update_property(&[PropertyLabel("Album cover art")]);
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
        .title("Cover")
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
pub fn start_cover_batch_decode(
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
#[must_use]
pub fn build_artist_icon_column() -> ColumnViewColumn {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, item: &Object| {
        let image = Image::builder()
            .icon_name("avatar-default-symbolic")
            .pixel_size(32)
            .width_request(32)
            .height_request(32)
            .css_classes(["artist-avatar", "dim-label"])
            .build();
        image.update_property(&[PropertyLabel("Artist icon")]);
        if let Some(list_item) = item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&image.upcast::<Widget>()));
        }
    });

    ColumnViewColumn::builder()
        .title("Icon")
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
pub fn build_string_column<T: Clone + Send + 'static>(
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
pub fn build_int_column<T: Clone + Send + 'static>(
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
#[must_use]
pub fn default_int_format(n: i32) -> String {
    if n == 0 { String::new() } else { n.to_string() }
}

#[cfg(test)]
mod tests {
    use crate::ui::library::columns::default_int_format;

    #[test]
    fn default_int_format_omits_zero() {
        assert_eq!(default_int_format(0), "");
        assert_eq!(default_int_format(12), "12");
        assert_eq!(default_int_format(2008), "2008");
    }
}

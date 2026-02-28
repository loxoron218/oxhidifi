//! Album audio-related column setup functions.

use libadwaita::{
    gio::File,
    glib::{BoxedAnyObject, Object},
    gtk::{
        ColumnView, ColumnViewColumn,
        ContentFit::Cover,
        CustomSorter, Label, ListItem, ListItemFactory,
        Ordering::{self, Equal, Larger, Smaller},
        Picture, SignalListItemFactory,
        pango::EllipsizeMode::End,
    },
    prelude::{Cast, ListItemExt, WidgetExt},
};

use crate::{library::models::Album, ui::formatting::format_sample_rate};

/// Sets up the cover art column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_cover_art_column(column_view: &ColumnView, fixed_width: i32) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(move |_, list_item| {
        let picture = Picture::builder()
            .content_fit(Cover)
            .width_request(fixed_width)
            .height_request(fixed_width)
            .build();

        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&picture));
        }
    });

    factory.connect_bind(|_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            let Some(child) = list_item.child() else {
                return;
            };
            let Some(picture) = child.downcast_ref::<Picture>() else {
                return;
            };
            let Some(boxed) = list_item.item() else {
                return;
            };
            let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>() else {
                return;
            };
            let album = album_obj.borrow::<Album>();
            if let Some(path) = &album.artwork_path {
                let file = File::for_path(path);
                picture.set_file(Some(&file));
            } else {
                picture.set_file(None::<&File>);
            }
        }
    });

    let column = ColumnViewColumn::new(None::<&str>, Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_expand(false);
    column.set_resizable(false);
    column_view.append_column(&column);
}

/// Sets up the sample rate column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_sample_rate_column(column_view: &ColumnView, fixed_width: i32) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let label = Label::builder().ellipsize(End).xalign(0.0).build();
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&label));
        }
    });

    factory.connect_bind(|_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(label) = child.downcast_ref::<Label>()
            && let Some(boxed) = list_item.item()
            && let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let album = album_obj.borrow::<Album>();
            if let Some(sample_rate) = album.sample_rate {
                label.set_text(&format_sample_rate(sample_rate));
                label.set_visible(true);
            } else {
                label.set_visible(false);
            }
        }
    });

    let column = ColumnViewColumn::new(
        Some("Sample Rate"),
        Some(factory.upcast::<ListItemFactory>()),
    );
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    let sorter = CustomSorter::new(|item1, item2| {
        let extract_sample_rate = |item: &Object| -> Option<i64> {
            item.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
                let album = boxed.borrow::<Album>();
                album.sample_rate
            })
        };
        let val1 = extract_sample_rate(item1);
        let val2 = extract_sample_rate(item2);
        match (val1, val2) {
            (Some(n1), Some(n2)) => Ordering::from(n1.cmp(&n2)),
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => Equal,
        }
    });
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

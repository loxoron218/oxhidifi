//! Artist column definitions for column view.
//!
//! This module provides factory functions for creating the 3 artist columns
//! in the column view, using GTK4's `SignalListItemFactory` pattern.

use std::sync::Arc;

use libadwaita::{
    glib::{BoxedAnyObject, Object},
    gtk::{
        Align::Center,
        ColumnView, ColumnViewColumn, CustomSorter, Image, Label, ListItem, ListItemFactory,
        Ordering::{self, Equal, Larger, Smaller},
        SignalListItemFactory,
        pango::EllipsizeMode::End,
    },
    prelude::{Cast, ListItemExt, WidgetExt},
};

use crate::{library::models::Artist, ui::views::column_view_types::ColumnListViewConfig};

/// Sets up all 3 artist columns for the column view.
///
/// # Arguments
///
/// * `column_view` - Column view to add columns to
/// * `_config` - Configuration options for the column view
pub fn setup_artist_columns(column_view: &mut ColumnView, _config: &ColumnListViewConfig) {
    setup_artist_cover_art_column(column_view, 48);
    setup_artist_name_column(column_view);
    setup_album_count_column(column_view, 100);
}

/// Sets up the artist cover art column (column 1).
/// Displays a symbolic icon since artists don't have cover art.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
fn setup_artist_cover_art_column(column_view: &ColumnView, fixed_width: i32) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(move |_, list_item| {
        let icon = Image::builder()
            .icon_name("avatar-default-symbolic")
            .pixel_size(fixed_width)
            .build();
        icon.set_halign(Center);
        icon.set_valign(Center);
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&icon));
        }
    });

    let column = ColumnViewColumn::new(None::<&str>, Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_expand(false);
    column.set_resizable(false);
    column_view.append_column(&column);
}

/// Sets up the artist name column (column 2).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
fn setup_artist_name_column(column_view: &ColumnView) {
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
            && let Ok(artist_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let artist = artist_obj.borrow::<Arc<Artist>>();
            label.set_text(&artist.name);
            label.set_visible(true);
        }
    });

    let column = ColumnViewColumn::new(Some("Artist"), Some(factory.upcast::<ListItemFactory>()));
    column.set_resizable(true);
    column.set_expand(true);
    let sorter = CustomSorter::new(|item1, item2| {
        let extract_artist = |item: &Object| -> Option<Arc<Artist>> {
            item.downcast_ref::<BoxedAnyObject>().map(|boxed| {
                let artist_ref = boxed.borrow::<Arc<Artist>>();
                Arc::clone(&artist_ref)
            })
        };

        let Some(arc_artist1) = extract_artist(item1) else {
            return Equal;
        };
        let Some(arc_artist2) = extract_artist(item2) else {
            return Equal;
        };

        let val1 = Some(&arc_artist1.name);
        let val2 = Some(&arc_artist2.name);
        match (val1, val2) {
            (Some(s1), Some(s2)) => {
                Ordering::from(s1.to_ascii_lowercase().cmp(&s2.to_ascii_lowercase()))
            }
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => Equal,
        }
    });
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the album count column (column 3).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
fn setup_album_count_column(column_view: &ColumnView, fixed_width: i32) {
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
            && let Ok(artist_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let artist = artist_obj.borrow::<Arc<Artist>>();
            label.set_text(&artist.album_count.to_string());
            label.set_visible(true);
        }
    });

    let column = ColumnViewColumn::new(Some("Albums"), Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    let sorter = CustomSorter::new(|item1, item2| {
        let get_count = |item: &Object| -> Option<i64> {
            item.downcast_ref::<BoxedAnyObject>().map(|boxed| {
                let artist = boxed.borrow::<Arc<Artist>>();
                artist.album_count
            })
        };
        let val1 = get_count(item1);
        let val2 = get_count(item2);
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

//! Album text column setup functions.
//!
//! This module provides factory functions for creating simple text-based album columns
//! in the column view, using GTK4's `SignalListItemFactory` pattern.

use libadwaita::{
    glib::BoxedAnyObject,
    gtk::{
        ColumnView, ColumnViewColumn, Label, ListItem, ListItemFactory, SignalListItemFactory,
        pango::EllipsizeMode::End,
    },
    prelude::{Cast, ListItemExt, WidgetExt},
};

use crate::{label_column, library::Album, ui::views::column_view_types::ArtistNameCache};

/// Sets up the album title column (column 2).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
pub fn setup_title_column(column_view: &mut ColumnView) {
    let column = label_column!(
        "Album",
        |album: &Album| Some(album.title.clone()),
        true,
        None::<i32>
    );
    column_view.append_column(&column);
}

/// Sets up the artist name column (column 3).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `artist_name_cache` - Cache of artist names
/// * `fixed_width` - Fixed width for the column
pub fn setup_artist_column(
    column_view: &mut ColumnView,
    artist_name_cache: &ArtistNameCache,
    fixed_width: i32,
) {
    let factory = SignalListItemFactory::new();
    let cache_clone = artist_name_cache.clone();

    factory.connect_setup(|_, list_item| {
        let label = Label::builder().ellipsize(End).xalign(0.0).build();
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&label));
        }
    });

    factory.connect_bind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(label) = child.downcast_ref::<Label>()
            && let Some(boxed) = list_item.item()
            && let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let album = album_obj.borrow::<Album>();
            let cache = cache_clone.borrow();
            let artist_name = cache.get(&album.artist_id).cloned();
            if let Some(name) = artist_name {
                label.set_text(&name);
                label.set_visible(true);
            } else {
                label.set_visible(false);
            }
        }
    });

    let column = ColumnViewColumn::new(Some("Artist"), Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    column_view.append_column(&column);
}

/// Sets up the year column (column 4).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_year_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Year",
        |album: &Album| album.year.map(|y| y.to_string()),
        true,
        Some(fixed_width)
    );
    column_view.append_column(&column);
}

/// Sets up the genre column (column 5).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_genre_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Genre",
        |album: &Album| album.genre.clone(),
        true,
        Some(fixed_width)
    );
    column_view.append_column(&column);
}

/// Sets up the track count column (column 6).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_track_count_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Tracks",
        |album: &Album| Some(album.track_count.to_string()),
        true,
        Some(fixed_width)
    );
    column_view.append_column(&column);
}

/// Sets up the bit depth column (column 7).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_bit_depth_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Bit Depth",
        |album: &Album| album.bits_per_sample.map(|b| b.to_string()),
        true,
        Some(fixed_width)
    );
    column_view.append_column(&column);
}

/// Sets up the channels column (column 9).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_channels_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Channels",
        |album: &Album| album.channels.map(|c| c.to_string()),
        true,
        Some(fixed_width)
    );
    column_view.append_column(&column);
}

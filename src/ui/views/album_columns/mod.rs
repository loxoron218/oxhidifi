//! Album column definitions for column view.
//!
//! This module provides factory functions for creating album columns
//! in the column view, using GTK4's `SignalListItemFactory` pattern.

use std::sync::Arc;

use libadwaita::{
    glib::{BoxedAnyObject, Object},
    gtk::{
        ColumnView, ColumnViewColumn, CustomSorter, Label, ListItem, ListItemFactory,
        Ordering::{self, Equal, Larger, Smaller},
        SignalListItemFactory,
        pango::EllipsizeMode::End,
    },
    prelude::{Cast, ListItemExt, WidgetExt},
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    label_column,
    library::{database::LibraryDatabase, models::Album},
    state::app_state::AppState,
    ui::views::column_view_types::ArtistNameCache,
};

mod audio_columns;
mod playback_columns;
mod sorters;

/// Sets up the cover art column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_cover_art_column(column_view: &mut ColumnView, fixed_width: i32) {
    audio_columns::setup_cover_art_column(column_view, fixed_width);
}

/// Sets up the sample rate column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_sample_rate_column(column_view: &mut ColumnView, fixed_width: i32) {
    audio_columns::setup_sample_rate_column(column_view, fixed_width);
}

/// Sets up the DR badge column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
/// * `show_dr_badges` - Whether to show DR badges
pub fn setup_dr_column(column_view: &mut ColumnView, fixed_width: i32, show_dr_badges: bool) {
    playback_columns::setup_dr_column(column_view, fixed_width, show_dr_badges);
}

/// Sets up the play button column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `library_db` - Optional library database
/// * `audio_engine` - Optional audio engine
/// * `queue_manager` - Optional queue manager
/// * `app_state` - Optional app state for updating UI
/// * `fixed_width` - Fixed width for the column
pub fn setup_play_button_column(
    column_view: &mut ColumnView,
    library_db: Option<&Arc<LibraryDatabase>>,
    audio_engine: Option<&Arc<AudioEngine>>,
    queue_manager: Option<&Arc<QueueManager>>,
    app_state: Option<&Arc<AppState>>,
    fixed_width: i32,
) {
    playback_columns::setup_play_button_column(
        column_view,
        library_db,
        audio_engine,
        queue_manager,
        app_state,
        fixed_width,
    );
}

/// Creates a string-based sorter for album columns.
///
/// # Arguments
///
/// * `get_value` - Function to extract the string value from an album
///
/// # Returns
///
/// A `CustomSorter` for sorting albums by string values
fn create_string_sorter(get_value: fn(&Album) -> Option<String>) -> CustomSorter {
    sorters::create_string_sorter(get_value)
}

/// Creates a numeric-based sorter for album columns.
///
/// # Arguments
///
/// * `get_value` - Function to extract the numeric value from an album
///
/// # Returns
///
/// A `CustomSorter` for sorting albums by numeric values
fn create_numeric_sorter(get_value: fn(&Album) -> Option<i64>) -> CustomSorter {
    sorters::create_numeric_sorter(get_value)
}

/// Sets up the album title column.
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
    let sorter = create_string_sorter(|album| Some(album.title.clone()));
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the artist name column.
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
    let cache_for_sort = artist_name_cache.clone();
    let sorter = CustomSorter::new(move |item1, item2| {
        let get_artist_name = |item: &Object| -> Option<String> {
            item.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
                let album = boxed.borrow::<Album>();
                let cache = cache_for_sort.borrow();
                cache.get(&album.artist_id).cloned()
            })
        };
        let val1 = get_artist_name(item1);
        let val2 = get_artist_name(item2);
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

/// Sets up the year column.
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
    let sorter = create_numeric_sorter(|album| album.year);
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the genre column.
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
    let sorter = create_string_sorter(|album| album.genre.clone());
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the track count column.
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
    let sorter = create_numeric_sorter(|album| Some(album.track_count));
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the bit depth column.
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
    let sorter = create_numeric_sorter(|album| album.bits_per_sample);
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the channels column.
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
    let sorter = create_numeric_sorter(|album| album.channels);
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up all 11 album columns for the column view.
///
/// # Arguments
///
/// * `column_view` - Column view to add columns to
/// * `artist_name_cache` - Cache of artist names for lookup
/// * `library_db` - Optional library database for fetching tracks
/// * `audio_engine` - Optional audio engine for playback
/// * `queue_manager` - Optional queue manager for queue operations
/// * `app_state` - Optional app state for updating UI
/// * `show_dr_badges` - Whether to show DR badges
pub fn setup_album_columns(
    column_view: &mut ColumnView,
    artist_name_cache: &ArtistNameCache,
    library_db: Option<&Arc<LibraryDatabase>>,
    audio_engine: Option<&Arc<AudioEngine>>,
    queue_manager: Option<&Arc<QueueManager>>,
    app_state: Option<&Arc<AppState>>,
    show_dr_badges: bool,
) {
    setup_cover_art_column(column_view, 48);
    setup_title_column(column_view);
    setup_artist_column(column_view, artist_name_cache, 200);
    setup_year_column(column_view, 60);
    setup_genre_column(column_view, 120);
    setup_track_count_column(column_view, 72);
    setup_bit_depth_column(column_view, 80);
    setup_sample_rate_column(column_view, 100);
    setup_channels_column(column_view, 80);
    setup_dr_column(column_view, 60, show_dr_badges);
    setup_play_button_column(
        column_view,
        library_db,
        audio_engine,
        queue_manager,
        app_state,
        48,
    );
}

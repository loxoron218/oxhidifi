//! Album column definitions for column view.
//!
//! This module provides factory functions for creating album columns
//! in the column view, using GTK4's `SignalListItemFactory` pattern.

mod audio_columns;
mod playback_columns;
mod sorters;

use std::sync::Arc;

use libadwaita::{
    glib::{BoxedAnyObject, JoinHandle, Object},
    gtk::{
        Align::Center,
        CheckButton, ColumnView, ColumnViewColumn, CustomSorter, Label, ListItem, ListItemFactory,
        MultiSelection,
        Ordering::{Equal as GtkEqual, Larger, Smaller},
        SignalListItemFactory,
        pango::EllipsizeMode::End,
    },
    prelude::{Cast, CheckButtonExt, ListItemExt, ObjectExt, SelectionModelExt, WidgetExt},
};

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    label_column,
    library::{database::LibraryDatabase, models::Album},
    state::app_state::AppState,
    ui::views::{column_sorting::compare_ignore_ascii_case, column_view_types::ArtistNameCache},
};

/// Context for playback-related operations.
pub struct PlaybackContext<'a> {
    /// Library database for track lookups.
    pub library_db: Option<&'a Arc<LibraryDatabase>>,
    /// Audio engine for playback operations.
    pub audio_engine: Option<&'a Arc<AudioEngine>>,
    /// Queue manager for queue management.
    pub queue_manager: Option<&'a Arc<QueueManager>>,
    /// Application state for UI updates.
    pub app_state: Option<&'a Arc<AppState>>,
}

/// Sets up the selection column with checkboxes.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `selection_model` - Selection model for the column view
/// * `fixed_width` - Fixed width for the column
pub fn setup_selection_column(
    column_view: &mut ColumnView,
    selection_model: &MultiSelection,
    fixed_width: i32,
) {
    let factory = SignalListItemFactory::new();
    let selection_model = selection_model.clone();

    let selection_model_setup = selection_model;
    factory.connect_setup(move |_, list_item_obj| {
        let check_button = CheckButton::builder().halign(Center).valign(Center).build();

        let selection_model_clone = selection_model_setup.clone();
        let list_item_weak = list_item_obj.downgrade();

        check_button.connect_toggled(move |cb| {
            if let Some(list_item) = list_item_weak.upgrade() {
                let position = list_item.property::<u32>("position");
                let is_active = cb.is_active();

                if selection_model_clone.is_selected(position) != is_active {
                    if is_active {
                        selection_model_clone.select_item(position, false);
                    } else {
                        selection_model_clone.unselect_item(position);
                    }
                }
            }
        });

        // Manually track changes to the "selected" property if it exists.
        // We use connect_notify_local to avoid Send/Sync requirements for the closure.
        let checkbox_weak = check_button.downgrade();
        list_item_obj.connect_notify_local(Some("selected"), move |obj, _| {
            if let Some(checkbox) = checkbox_weak.upgrade()
                && let Ok(selected) = obj.property_value("selected").get::<bool>()
                && checkbox.is_active() != selected
            {
                checkbox.set_active(selected);
            }
        });

        // Use property access instead of downcast to ListItem to be safe with ColumnViewCell
        list_item_obj.set_property("child", Some(&check_button));
    });

    factory.connect_bind(move |_, list_item_obj| {
        if let Some(checkbox) = list_item_obj.property::<Option<CheckButton>>("child")
            && let Ok(selected) = list_item_obj.property_value("selected").get::<bool>()
        {
            checkbox.set_active(selected);
        }
    });

    let column = ColumnViewColumn::builder()
        .title("")
        .fixed_width(fixed_width)
        .resizable(false)
        .factory(&factory)
        .build();

    column_view.insert_column(0, &column);
}

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
/// * `playback_context` - Playback context with optional dependencies
/// * `fixed_width` - Fixed width for the column
///
/// # Returns
///
/// An optional join handle for the state subscription.
pub fn setup_play_button_column(
    column_view: &mut ColumnView,
    playback_context: &PlaybackContext<'_>,
    fixed_width: i32,
) -> Option<JoinHandle<()>> {
    playback_columns::setup_play_button_column(
        column_view,
        playback_context.library_db,
        playback_context.audio_engine,
        playback_context.queue_manager,
        playback_context.app_state,
        fixed_width,
    )
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
fn create_string_sorter(get_value: fn(&Album) -> Option<&String>) -> CustomSorter {
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
        Album,
        |album: &Album| Some(album.title.clone()),
        true,
        None::<i32>
    );
    let sorter = create_string_sorter(|album| Some(&album.title));
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
            let album = album_obj.borrow::<Arc<Album>>();
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
        let extract_album = |item: &Object| -> Option<Arc<Album>> {
            item.downcast_ref::<BoxedAnyObject>().map(|boxed| {
                let album_ref = boxed.borrow::<Arc<Album>>();
                Arc::clone(&album_ref)
            })
        };

        let Some(arc_album1) = extract_album(item1) else {
            return GtkEqual;
        };
        let Some(arc_album2) = extract_album(item2) else {
            return GtkEqual;
        };

        let cache = cache_for_sort.borrow();
        let val1 = cache.get(&arc_album1.artist_id);
        let val2 = cache.get(&arc_album2.artist_id);
        match (val1, val2) {
            (Some(s1), Some(s2)) => compare_ignore_ascii_case(s1, s2),
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => GtkEqual,
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
        Album,
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
        Album,
        |album: &Album| album.genre.clone(),
        true,
        Some(fixed_width)
    );
    let sorter = create_string_sorter(|album| album.genre.as_ref());
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
        Album,
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
        Album,
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
        Album,
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
/// * `selection_model` - Selection model for the column view
/// * `artist_name_cache` - Cache of artist names for lookup
/// * `playback_context` - Playback context with optional dependencies
/// * `show_dr_badges` - Whether to show DR badges
///
/// # Returns
///
/// An optional join handle for the play button state subscription.
pub fn setup_album_columns(
    column_view: &mut ColumnView,
    selection_model: &MultiSelection,
    artist_name_cache: &ArtistNameCache,
    playback_context: &PlaybackContext<'_>,
    show_dr_badges: bool,
) -> Option<JoinHandle<()>> {
    setup_selection_column(column_view, selection_model, 40);
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
    setup_play_button_column(column_view, playback_context, 48)
}

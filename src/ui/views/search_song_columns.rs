//! Column definitions for search results song `ColumnView`.
//!
//! This module provides factory functions for creating track columns in the
//! search results column view, using GTK4's `SignalListItemFactory` pattern
//! with `TrackSearchResult` data.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use {
    libadwaita::{
        glib::{BoxedAnyObject, JoinHandle, MainContext, Object},
        gtk::{
            Align::Center,
            Button, ColumnView, ColumnViewColumn, CustomSorter, Label, ListItem, ListItemFactory,
            Ordering::{self as GtkOrdering, Equal as GtkEqual, Larger, Smaller},
            SignalListItemFactory,
            pango::EllipsizeMode::End,
        },
        prelude::{ButtonExt, Cast, ListItemExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    audio::{
        constants::{DEFAULT_BIT_DEPTH, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE},
        decoder_types::AudioFormat,
        engine::{AudioEngine, PlaybackState::Playing, TrackInfo},
        metadata::TagReader,
        queue_manager::QueueManager,
    },
    label_column,
    library::{database::LibraryDatabase, models::TrackSearchResult},
    state::app_state::AppState,
    ui::{
        components::hifi_metadata::{
            BitDepthDisplay::Show as ShowBitDepth, ChannelsDisplay::Hide as HideChannels,
            FormatDisplay::Show as ShowFormat, HiFiMetadata, LayoutMode::Compact,
            SampleRateDisplay::Show as ShowSampleRate,
        },
        views::{
            column_sorting::compare_ignore_ascii_case, search_results_view::SONG_DISPLAY_LIMIT,
        },
    },
};

/// Sets up the track number column with sorting support.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_track_number_column(column_view: &mut ColumnView, fixed_width: i32) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let label = Label::builder()
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&label));
        }
    });

    factory.connect_bind(|_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(label) = child.downcast_ref::<Label>()
            && let Some(boxed) = list_item.item()
            && let Ok(obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let result = obj.borrow::<Arc<TrackSearchResult>>();
            let num = result.track.track_number.unwrap_or(0);
            label.set_text(&num.to_string());
        }
    });

    let column = ColumnViewColumn::new(Some("#"), Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(false);
    let sorter = CustomSorter::new(|item1, item2| {
        let extract = |item: &Object| -> i64 {
            item.downcast_ref::<BoxedAnyObject>().map_or(0, |boxed| {
                let result = boxed.borrow::<Arc<TrackSearchResult>>();
                result.track.track_number.unwrap_or(0)
            })
        };
        GtkOrdering::from(extract(item1).cmp(&extract(item2)))
    });
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the title column with sorting support.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
pub fn setup_title_column(column_view: &mut ColumnView) {
    let column = label_column!(
        "Title",
        TrackSearchResult,
        |result: &TrackSearchResult| Some(result.track.title.clone()),
        true,
        None::<i32>
    );
    let sorter = create_string_sorter(|result| Some(&result.track.title));
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the artist column with sorting support.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_artist_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Artist",
        TrackSearchResult,
        |result: &TrackSearchResult| Some(result.artist_name.clone()),
        true,
        Some(fixed_width)
    );
    let sorter = create_string_sorter(|result| Some(&result.artist_name));
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the album column with sorting support.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_album_column(column_view: &mut ColumnView, fixed_width: i32) {
    let column = label_column!(
        "Album",
        TrackSearchResult,
        |result: &TrackSearchResult| Some(result.album_title.clone()),
        true,
        Some(fixed_width)
    );
    let sorter = create_string_sorter(|result| Some(&result.album_title));
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the duration column with MM:SS format and sorting support.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_duration_column(column_view: &mut ColumnView, fixed_width: i32) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let label = Label::builder()
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&label));
        }
    });

    factory.connect_bind(|_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(label) = child.downcast_ref::<Label>()
            && let Some(boxed) = list_item.item()
            && let Ok(obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let result = obj.borrow::<Arc<TrackSearchResult>>();
            let secs = result.track.duration_ms / 1000;
            let mins = secs / 60;
            let rem = secs % 60;
            label.set_text(&format!("{mins:02}:{rem:02}"));
        }
    });

    let column = ColumnViewColumn::new(Some("Duration"), Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    let sorter = CustomSorter::new(|item1, item2| {
        let extract = |item: &Object| -> i64 {
            item.downcast_ref::<BoxedAnyObject>().map_or(0, |boxed| {
                let result = boxed.borrow::<Arc<TrackSearchResult>>();
                result.track.duration_ms
            })
        };
        GtkOrdering::from(extract(item1).cmp(&extract(item2)))
    });
    column.set_sorter(Some(&sorter));
    column_view.append_column(&column);
}

/// Sets up the `HiFi` metadata column showing format, sample rate, and bit depth.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
pub fn setup_hifi_metadata_column(column_view: &mut ColumnView, fixed_width: i32) {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let label = Label::builder()
            .ellipsize(End)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&label));
        }
    });

    factory.connect_bind(|_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(label) = child.downcast_ref::<Label>()
            && let Some(boxed) = list_item.item()
            && let Ok(obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let result = obj.borrow::<Arc<TrackSearchResult>>();
            let track = &result.track;

            let metadata = HiFiMetadata::builder()
                .track(track.clone())
                .show_format(ShowFormat)
                .show_sample_rate(ShowSampleRate)
                .show_bit_depth(ShowBitDepth)
                .show_channels(HideChannels)
                .layout(Compact)
                .build();

            let parts: Vec<String> = metadata
                .labels
                .iter()
                .map(|l| l.text().to_string())
                .collect();

            label.set_text(&parts.join(" "));
        }
    });

    let column = ColumnViewColumn::new(Some("Quality"), Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    column_view.append_column(&column);
}

/// Sets up the play button column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `library_db` - Optional library database for track lookups
/// * `audio_engine` - Optional audio engine for playback
/// * `queue_manager` - Optional queue manager for queue operations
/// * `app_state` - Optional application state for UI updates
/// * `fixed_width` - Fixed width for the column
///
/// # Returns
///
/// An optional join handle for the state subscription.
pub fn setup_play_button_column(
    column_view: &mut ColumnView,
    library_db: Option<&Arc<LibraryDatabase>>,
    audio_engine: Option<&Arc<AudioEngine>>,
    queue_manager: Option<&Arc<QueueManager>>,
    app_state: Option<&Arc<AppState>>,
    fixed_width: i32,
) -> Option<JoinHandle<()>> {
    let factory = SignalListItemFactory::new();

    let library_db_clone = library_db.cloned();
    let audio_engine_clone = audio_engine.cloned();
    let queue_manager_clone = queue_manager.cloned();
    let app_state_clone = app_state.cloned();

    let buttons_map: Rc<RefCell<HashMap<String, Button>>> =
        Rc::new(RefCell::new(HashMap::with_capacity(SONG_DISPLAY_LIMIT)));

    factory.connect_setup(move |_, list_item| {
        let button = Button::builder()
            .icon_name("media-playback-start-symbolic")
            .css_classes(["flat"])
            .has_frame(false)
            .tooltip_text("Play track")
            .build();
        button.set_halign(Center);
        button.set_valign(Center);

        let library_db_for_cb = library_db_clone.clone();
        let audio_engine_for_cb = audio_engine_clone.clone();
        let queue_manager_for_cb = queue_manager_clone.clone();
        let app_state_for_cb = app_state_clone.clone();

        button.connect_clicked(move |btn| {
            let name = btn.widget_name();
            let parts: Vec<&str> = name.split('|').collect();
            if parts.len() != 2 {
                error!(widget_name = %name, "Failed to parse track info from widget name");
                return;
            }

            let Ok(album_id) = parts[0].parse::<i64>() else {
                error!(widget_name = %name, "Failed to parse album ID from widget name");
                return;
            };
            let track_path = decode_path_from_widget_name(parts[1]);

            handle_play_button_click(
                album_id,
                track_path,
                library_db_for_cb.clone(),
                audio_engine_for_cb.clone(),
                queue_manager_for_cb.clone(),
                app_state_for_cb.clone(),
            );
        });

        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&button));
        }
    });

    let buttons_map_bind = Rc::clone(&buttons_map);
    let app_state_for_bind = app_state.cloned();

    factory.connect_bind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(button) = child.downcast_ref::<Button>()
            && let Some(boxed) = list_item.item()
            && let Ok(obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let result = obj.borrow::<Arc<TrackSearchResult>>();
            let encoded_path = encode_path_for_widget_name(&result.track.path);
            let widget_name = format!("{}|{encoded_path}", result.album_id);
            button.set_widget_name(&widget_name);

            buttons_map_bind
                .borrow_mut()
                .insert(result.track.path.clone(), button.clone());

            update_button_state(button, &result.track.path, app_state_for_bind.as_ref());
        }
    });

    let buttons_map_unbind = Rc::clone(&buttons_map);
    factory.connect_unbind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(boxed) = list_item.item()
            && let Ok(obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let result = obj.borrow::<Arc<TrackSearchResult>>();
            buttons_map_unbind.borrow_mut().remove(&result.track.path);
        }
    });

    let subscription_handle =
        app_state.map(|state| spawn_state_subscription(Arc::clone(state), buttons_map));

    let column = ColumnViewColumn::new(None::<&str>, Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(false);
    column_view.append_column(&column);

    subscription_handle
}

/// Sets up all columns for the search results song column view.
///
/// # Arguments
///
/// * `column_view` - Column view to add columns to
/// * `library_db` - Optional library database for track lookups
/// * `audio_engine` - Optional audio engine for playback
/// * `queue_manager` - Optional queue manager for queue operations
/// * `app_state` - Optional application state for UI updates
///
/// # Returns
///
/// An optional join handle for the play button state subscription.
pub fn setup_search_song_columns(
    column_view: &mut ColumnView,
    library_db: Option<&Arc<LibraryDatabase>>,
    audio_engine: Option<&Arc<AudioEngine>>,
    queue_manager: Option<&Arc<QueueManager>>,
    app_state: Option<&Arc<AppState>>,
) -> Option<JoinHandle<()>> {
    setup_track_number_column(column_view, 40);
    setup_title_column(column_view);
    setup_artist_column(column_view, 200);
    setup_album_column(column_view, 200);
    setup_duration_column(column_view, 72);
    setup_hifi_metadata_column(column_view, 200);
    setup_play_button_column(
        column_view,
        library_db,
        audio_engine,
        queue_manager,
        app_state,
        48,
    )
}

/// Handles play button click by spawning async playback.
///
/// Extracts album ID and track path from the widget name, then spawns
/// an async task to play the track from its album context.
///
/// # Arguments
///
/// * `album_id` - Database ID of the album containing the track
/// * `track_path` - File system path to the track
/// * `library_db` - Optional library database for track lookups
/// * `audio_engine` - Optional audio engine for playback
/// * `queue_manager` - Optional queue manager for queue operations
/// * `app_state` - Optional application state for UI updates
fn handle_play_button_click(
    album_id: i64,
    track_path: String,
    library_db: Option<Arc<LibraryDatabase>>,
    audio_engine: Option<Arc<AudioEngine>>,
    queue_manager: Option<Arc<QueueManager>>,
    app_state: Option<Arc<AppState>>,
) {
    MainContext::default().spawn_local(async move {
        play_track_from_album(
            library_db,
            audio_engine,
            queue_manager,
            app_state,
            &track_path,
            album_id,
        )
        .await;
    });
}

/// Plays a specific track from its album, queuing the full album.
///
/// Fetches all tracks from the album, sets them as the playback queue,
/// loads the specified track, starts playback, and updates application state.
///
/// # Arguments
///
/// * `library_db` - Library database for track lookups
/// * `audio_engine` - Audio engine for playback control
/// * `queue_manager` - Queue manager for queue operations
/// * `app_state` - Application state for UI updates
/// * `track_path` - File system path to the target track
/// * `album_id` - Database ID of the album
async fn play_track_from_album(
    library_db: Option<Arc<LibraryDatabase>>,
    audio_engine: Option<Arc<AudioEngine>>,
    queue_manager: Option<Arc<QueueManager>>,
    app_state: Option<Arc<AppState>>,
    track_path: &str,
    album_id: i64,
) {
    let (Some(db), Some(engine), Some(qm), Some(state)) =
        (library_db, audio_engine, queue_manager, app_state)
    else {
        return;
    };

    let album_tracks = match db.get_tracks_by_album(album_id).await {
        Ok(tracks) if !tracks.is_empty() => tracks,
        Ok(_) => return,
        Err(e) => {
            error!(album_id = album_id, error = %e, "Failed to fetch album tracks");
            return;
        }
    };

    qm.set_queue(album_tracks.clone());

    let track_index = album_tracks
        .iter()
        .position(|t| t.path == track_path)
        .unwrap_or(0);

    let Some(target_track) = album_tracks.get(track_index) else {
        return;
    };

    if let Err(e) = engine.load_track(&target_track.path) {
        error!(track_path = %target_track.path, error = %e, "Failed to load track");
        return;
    }

    if let Err(e) = engine.play().await {
        error!(error = %e, "Failed to start playback");
        return;
    }

    state.update_playback_state(Playing);
    state.update_current_album_id(Some(album_id));

    if let Ok(metadata) = TagReader::read_metadata(&target_track.path) {
        let track_info = TrackInfo {
            path: target_track.path.clone(),
            metadata,
            format: AudioFormat {
                sample_rate: u32::try_from(target_track.sample_rate).unwrap_or(DEFAULT_SAMPLE_RATE),
                channels: u32::try_from(target_track.channels).unwrap_or(DEFAULT_CHANNELS),
                bits_per_sample: u32::try_from(target_track.bits_per_sample)
                    .unwrap_or(DEFAULT_BIT_DEPTH),
                channel_mask: 0,
            },
            duration_ms: u64::try_from(target_track.duration_ms).unwrap_or(0),
        };
        state.update_current_track(Some(track_info));
    }
}

/// Updates a play button's icon based on the current playback state.
fn update_button_state(button: &Button, track_path: &str, app_state: Option<&Arc<AppState>>) {
    if let Some(state) = app_state {
        let is_playing = state.get_playback_state() == Playing;
        let current_path = state.get_current_track().map(|t| t.path);
        let is_current = current_path.as_deref() == Some(track_path);

        if is_current && is_playing {
            button.set_icon_name("media-playback-pause-symbolic");
            button.set_tooltip_text(Some("Pause"));
        } else {
            button.set_icon_name("media-playback-start-symbolic");
            button.set_tooltip_text(Some("Play track"));
        }
    }
}

/// Spawns async task to subscribe to app state changes and update buttons.
fn spawn_state_subscription(
    app_state: Arc<AppState>,
    buttons_map: Rc<RefCell<HashMap<String, Button>>>,
) -> JoinHandle<()> {
    MainContext::default().spawn_local(async move {
        use crate::state::app_state::AppStateEvent::{
            CurrentTrackChanged, PlaybackStateChanged, QueueChanged,
        };

        let rx = app_state.subscribe();
        while let Ok(event) = rx.recv().await {
            if matches!(
                &*event,
                CurrentTrackChanged(_) | PlaybackStateChanged(_) | QueueChanged(_)
            ) {
                let is_playing = app_state.get_playback_state() == Playing;
                let current_path = app_state.get_current_track().map(|t| t.path);

                let buttons = buttons_map.borrow();
                for (path, button) in buttons.iter() {
                    let is_current = current_path.as_deref() == Some(path.as_str());
                    if is_current && is_playing {
                        button.set_icon_name("media-playback-pause-symbolic");
                        button.set_tooltip_text(Some("Pause"));
                    } else {
                        button.set_icon_name("media-playback-start-symbolic");
                        button.set_tooltip_text(Some("Play track"));
                    }
                }
            }
        }
    })
}

/// Percent-encodes a path string for safe use in GTK widget names.
///
/// The widget name format is `{album_id}|{encoded_path}`, so the path
/// must be encoded to avoid collision with the separator character.
///
/// # Arguments
///
/// * `path` - The file system path to encode
///
/// # Returns
///
/// The percent-encoded path
fn encode_path_for_widget_name(path: &str) -> String {
    path.replace('|', "%7C")
}

/// Decodes a percent-encoded path from a widget name.
///
/// # Arguments
///
/// * `encoded` - The percent-encoded path string
///
/// # Returns
///
/// The decoded file system path
fn decode_path_from_widget_name(encoded: &str) -> String {
    encoded.replace("%7C", "|")
}

/// Creates a case-insensitive string sorter for `TrackSearchResult` items.
///
/// # Arguments
///
/// * `get_value` - Function to extract the string field to sort by
///
/// # Returns
///
/// A `CustomSorter` configured for case-insensitive string sorting
fn create_string_sorter(get_value: fn(&TrackSearchResult) -> Option<&String>) -> CustomSorter {
    CustomSorter::new(move |item1, item2| {
        let boxed1 = item1.downcast_ref::<BoxedAnyObject>();
        let boxed2 = item2.downcast_ref::<BoxedAnyObject>();

        match (boxed1, boxed2) {
            (Some(b1), Some(b2)) => {
                let r1: Arc<TrackSearchResult> = b1.borrow::<Arc<TrackSearchResult>>().clone();
                let r2: Arc<TrackSearchResult> = b2.borrow::<Arc<TrackSearchResult>>().clone();
                let s1 = get_value(&r1);
                let s2 = get_value(&r2);

                match (s1, s2) {
                    (Some(v1), Some(v2)) => compare_ignore_ascii_case(v1, v2),
                    (Some(_), None) => Larger,
                    (None, Some(_)) => Smaller,
                    (None, None) => GtkEqual,
                }
            }
            (Some(_), None) => Larger,
            (None, Some(_)) => Smaller,
            (None, None) => GtkEqual,
        }
    })
}

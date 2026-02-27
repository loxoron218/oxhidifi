//! Album column definitions for column view.
//!
//! This module provides factory functions for creating album columns
//! in the column view, using GTK4's `SignalListItemFactory` pattern.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use {
    libadwaita::{
        gio::File,
        glib::{BoxedAnyObject, MainContext},
        gtk::{
            Align::Center, Button, ColumnView, ColumnViewColumn, ContentFit::Cover, Label,
            ListItem, ListItemFactory, Picture, SignalListItemFactory, pango::EllipsizeMode::End,
        },
        prelude::{ButtonExt, Cast, ListItemExt, ObjectType, WidgetExt},
    },
    tracing::error,
};

use crate::{
    audio::{
        decoder_types::AudioFormat,
        engine::{AudioEngine, PlaybackState::Playing, TrackInfo},
        metadata::TagReader,
        queue_manager::QueueManager,
    },
    library::{database::LibraryDatabase, models::Album},
    state::app_state::AppState,
    ui::{
        components::dr_badge::{DRBadge, DRQuality},
        formatting::format_sample_rate,
        views::{
            album_columns_text::{
                setup_artist_column, setup_bit_depth_column, setup_channels_column,
                setup_genre_column, setup_title_column, setup_track_count_column,
                setup_year_column,
            },
            column_view_types::ArtistNameCache,
        },
    },
};

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
    setup_track_count_column(column_view, 60);
    setup_bit_depth_column(column_view, 60);
    setup_sample_rate_column(column_view, 80);
    setup_channels_column(column_view, 60);
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

/// Sets up the cover art column (column 1).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
fn setup_cover_art_column(column_view: &ColumnView, fixed_width: i32) {
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

/// Sets up the sample rate column (column 8).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
fn setup_sample_rate_column(column_view: &ColumnView, fixed_width: i32) {
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
    column_view.append_column(&column);
}

/// Sets up the DR badge column (column 10).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
/// * `show_dr_badges` - Whether to show DR badges
fn setup_dr_column(column_view: &ColumnView, fixed_width: i32, show_dr_badges: bool) {
    let factory = SignalListItemFactory::new();

    let dr_badges: Rc<RefCell<HashMap<usize, DRBadge>>> = Rc::new(RefCell::new(HashMap::new()));

    let dr_badges_clone = dr_badges.clone();
    factory.connect_setup(move |_, list_item| {
        let dr_badge = DRBadge::new(None, false);
        let widget_clone = dr_badge.widget.clone();
        let widget_ptr = widget_clone.as_ptr() as usize;
        dr_badges_clone.borrow_mut().insert(widget_ptr, dr_badge);
        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&widget_clone));
        }
    });

    let dr_badges_clone = dr_badges;
    factory.connect_bind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(boxed) = list_item.item()
            && let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let album = album_obj.borrow::<Album>();
            let widget_ptr = child.as_ptr() as usize;
            if let Some(dr_badge) = dr_badges_clone.borrow_mut().get_mut(&widget_ptr) {
                let (display_text, css_class) = album.dr_value.as_deref().map_or_else(
                    || ("N/A".to_string(), "dr-na".to_string()),
                    |v| {
                        let numeric = v
                            .chars()
                            .skip_while(|c| !c.is_ascii_digit())
                            .collect::<String>();
                        if numeric.is_empty() {
                            ("N/A".to_string(), "dr-na".to_string())
                        } else {
                            let quality = DRQuality::from_dr_value(v);
                            (format!("{numeric:0>2}"), quality.css_class().to_string())
                        }
                    },
                );
                dr_badge.label.set_label(&display_text);
                dr_badge.label.set_css_classes(&[
                    "dr-badge-label",
                    "dr-badge-label-grid",
                    "tag",
                    &css_class,
                ]);
            }
            if show_dr_badges {
                child.set_visible(true);
            } else {
                child.set_visible(false);
            }
        }
    });

    let column = ColumnViewColumn::new(Some("DR"), Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    column_view.append_column(&column);
}

/// Sets up the play button column (column 11).
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `library_db` - Optional library database
/// * `audio_engine` - Optional audio engine
/// * `queue_manager` - Optional queue manager
/// * `app_state` - Optional app state for updating UI
/// * `fixed_width` - Fixed width for the column
///
/// # Panics
///
/// Panics if the widget name cannot be parsed as an album ID.
fn setup_play_button_column(
    column_view: &ColumnView,
    library_db: Option<&Arc<LibraryDatabase>>,
    audio_engine: Option<&Arc<AudioEngine>>,
    queue_manager: Option<&Arc<QueueManager>>,
    app_state: Option<&Arc<AppState>>,
    fixed_width: i32,
) {
    let factory = SignalListItemFactory::new();

    let library_db_clone = library_db.cloned();
    let audio_engine_clone = audio_engine.cloned();
    let queue_manager_clone = queue_manager.cloned();
    let app_state_clone = app_state.cloned();

    factory.connect_setup(move |_, list_item| {
        let button = Button::builder()
            .icon_name("media-playback-start-symbolic")
            .css_classes(["play-button"])
            .build();
        button.set_halign(Center);
        button.set_valign(Center);

        let library_db_for_cb = library_db_clone.clone();
        let audio_engine_for_cb = audio_engine_clone.clone();
        let queue_manager_for_cb = queue_manager_clone.clone();
        let app_state_for_cb = app_state_clone.clone();

        button.connect_clicked(move |button| {
            let Ok(album_id) = button.widget_name().parse::<i64>() else {
                error!("Failed to parse album-id from widget name");
                return;
            };

            let library_db = library_db_for_cb.clone();
            let audio_engine = audio_engine_for_cb.clone();
            let queue_manager = queue_manager_for_cb.clone();
            let app_state = app_state_for_cb.clone();

            MainContext::default().spawn_local(async move {
                if let (Some(db), Some(qm), Some(engine), Some(state)) =
                    (library_db, queue_manager, audio_engine, app_state)
                {
                    let tracks = match db.get_tracks_by_album(album_id).await {
                        Ok(t) => t,
                        Err(e) => {
                            error!("Failed to fetch tracks for album {}: {}", album_id, e);
                            return;
                        }
                    };

                    if tracks.is_empty() {
                        return;
                    }

                    qm.set_queue(tracks.clone());

                    let first_track = &tracks[0];
                    let track_path = &first_track.path;

                    if let Err(e) = engine.load_track(track_path) {
                        error!(error = %e, "Failed to load track: {}", track_path);
                        return;
                    }

                    if let Err(e) = engine.play().await {
                        error!("Failed to start playback: {}", e);
                        return;
                    }

                    state.update_playback_state(Playing);

                    if let Ok(metadata) = TagReader::read_metadata(track_path) {
                        let track_info = TrackInfo {
                            path: track_path.clone(),
                            metadata,
                            format: AudioFormat {
                                sample_rate: u32::try_from(first_track.sample_rate)
                                    .unwrap_or(44100),
                                channels: u32::try_from(first_track.channels).unwrap_or(2),
                                bits_per_sample: u32::try_from(first_track.bits_per_sample)
                                    .unwrap_or(16),
                                channel_mask: 0,
                            },
                            duration_ms: u64::try_from(first_track.duration_ms).unwrap_or(0),
                        };
                        state.update_current_track(Some(track_info));
                    }
                }
            });
        });

        if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
            list_item.set_child(Some(&button));
        }
    });

    factory.connect_bind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(button) = child.downcast_ref::<Button>()
            && let Some(boxed) = list_item.item()
            && let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let album = album_obj.borrow::<Album>();
            let album_id = album.id;
            button.set_widget_name(&album_id.to_string());
        }
    });

    let column = ColumnViewColumn::new(None::<&str>, Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(false);
    column_view.append_column(&column);
}

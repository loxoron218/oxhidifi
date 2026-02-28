//! Album playback-related column setup functions.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use {
    libadwaita::{
        glib::{BoxedAnyObject, MainContext, Object},
        gtk::{
            Align::Center,
            Button, ColumnView, ColumnViewColumn, CustomSorter, ListItem, ListItemFactory,
            Ordering::{self, Equal, Larger, Smaller},
            SignalListItemFactory,
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
    ui::components::dr_badge::{DRBadge, DRQuality},
};

/// Sets up the DR badge column.
///
/// # Arguments
///
/// * `column_view` - Column view to add column to
/// * `fixed_width` - Fixed width for the column
/// * `show_dr_badges` - Whether to show DR badges
pub fn setup_dr_column(column_view: &ColumnView, fixed_width: i32, show_dr_badges: bool) {
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
                let (display_text, css_class) = album.dr_value_numeric().map_or_else(
                    || ("N/A".to_string(), "dr-na".to_string()),
                    |numeric| {
                        let quality =
                            DRQuality::from_dr_value(album.dr_value.as_deref().unwrap_or_default());
                        (format!("{numeric:0>2}"), quality.css_class().to_string())
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
    let sorter = CustomSorter::new(|item1, item2| {
        let extract_dr = |item: &Object| -> Option<i64> {
            item.downcast_ref::<BoxedAnyObject>().and_then(|boxed| {
                let album = boxed.borrow::<Album>();
                album.dr_value_numeric()
            })
        };
        let val1 = extract_dr(item1);
        let val2 = extract_dr(item2);
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
///
/// # Panics
///
/// Panics if the widget name cannot be parsed as an album ID.
pub fn setup_play_button_column(
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
            .has_frame(false)
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

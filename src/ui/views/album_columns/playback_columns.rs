//! Album playback-related column setup functions.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use {
    libadwaita::{
        glib::{BoxedAnyObject, JoinHandle, MainContext, Object},
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
        engine::{AudioEngine, PlaybackState::Playing},
        queue_manager::QueueManager,
    },
    library::{database::LibraryDatabase, models::Album},
    state::app_state::{
        AppState,
        AppStateEvent::{CurrentTrackChanged, PlaybackStateChanged, QueueChanged},
    },
    ui::{
        components::dr_badge::{DRBadge, DRQuality},
        views::detail_playback::play_album,
    },
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
            let album = album_obj.borrow::<Arc<Album>>();
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
                let album = boxed.borrow::<Arc<Album>>();
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

/// Handles play button click to start/pause album playback.
///
/// # Arguments
///
/// * `album_id` - Album ID to play
/// * `library_db` - Library database reference
/// * `audio_engine` - Audio engine reference
/// * `queue_manager` - Queue manager reference
/// * `app_state` - Application state reference
fn handle_play_button_click(
    album_id: i64,
    library_db: Option<Arc<LibraryDatabase>>,
    audio_engine: Option<Arc<AudioEngine>>,
    queue_manager: Option<Arc<QueueManager>>,
    app_state: Option<Arc<AppState>>,
) {
    MainContext::default().spawn_local(async move {
        play_album(album_id, library_db, audio_engine, queue_manager, app_state).await;
    });
}

/// Updates play button icon and tooltip based on current playback state.
///
/// # Arguments
///
/// * `button` - Button widget to update
/// * `album_id` - Album ID
/// * `app_state` - Application state reference
fn update_button_state(button: &Button, album_id: i64, app_state: Option<&Arc<AppState>>) {
    if let Some(state) = app_state {
        let playback_state = state.get_playback_state();
        let current_album_id = state.get_current_album_id();
        let is_current_album = current_album_id == Some(album_id);
        let should_show_pause = is_current_album && playback_state == Playing;

        if should_show_pause {
            button.set_icon_name("media-playback-pause-symbolic");
            button.set_tooltip_text(Some("Pause"));
        } else {
            button.set_icon_name("media-playback-start-symbolic");
            button.set_tooltip_text(Some("Play"));
        }
    }
}

/// Spawns async task to subscribe to app state changes and update buttons.
///
/// # Arguments
///
/// * `app_state` - Application state to subscribe to
/// * `buttons_map` - Map of album IDs to buttons
///
/// # Returns
///
/// A join handle for the subscription.
fn spawn_state_subscription(
    app_state: Arc<AppState>,
    buttons_map: Rc<RefCell<HashMap<i64, Button>>>,
) -> JoinHandle<()> {
    let previous_album_id: Rc<RefCell<Option<i64>>> = Rc::new(RefCell::new(None));
    MainContext::default().spawn_local(async move {
        let rx = app_state.subscribe();
        while let Ok(event) = rx.recv().await {
            if matches!(
                event,
                CurrentTrackChanged(_) | PlaybackStateChanged(_) | QueueChanged(_)
            ) {
                let is_playing = app_state.get_playback_state() == Playing;
                let current_album_id = app_state.get_current_album_id();

                let mut buttons = buttons_map.borrow_mut();
                if let Some(prev_id) = *previous_album_id.borrow()
                    && let Some(prev_button) = buttons.get_mut(&prev_id)
                {
                    prev_button.set_icon_name("media-playback-start-symbolic");
                    prev_button.set_tooltip_text(Some("Play"));
                }
                if let Some(album_id) = current_album_id
                    && let Some(current_button) = buttons.get_mut(&album_id)
                {
                    if is_playing {
                        current_button.set_icon_name("media-playback-pause-symbolic");
                        current_button.set_tooltip_text(Some("Pause"));
                    } else {
                        current_button.set_icon_name("media-playback-start-symbolic");
                        current_button.set_tooltip_text(Some("Play"));
                    }
                }
                *previous_album_id.borrow_mut() = current_album_id;
            }
        }
    })
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
/// # Returns
///
/// A join handle for the state subscription.
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
) -> Option<JoinHandle<()>> {
    let factory = SignalListItemFactory::new();

    let library_db_clone = library_db.cloned();
    let audio_engine_clone = audio_engine.cloned();
    let queue_manager_clone = queue_manager.cloned();
    let app_state_clone = app_state.cloned();

    let buttons_map: Rc<RefCell<HashMap<i64, Button>>> = Rc::new(RefCell::new(HashMap::new()));

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

            handle_play_button_click(
                album_id,
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

    let buttons_map_bind = buttons_map.clone();
    let app_state_for_bind = app_state.cloned();

    factory.connect_bind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(child) = list_item.child()
            && let Some(button) = child.downcast_ref::<Button>()
            && let Some(boxed) = list_item.item()
            && let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let album = album_obj.borrow::<Arc<Album>>();
            let album_id = album.id;
            button.set_widget_name(&album_id.to_string());

            buttons_map_bind
                .borrow_mut()
                .insert(album_id, button.clone());

            update_button_state(button, album_id, app_state_for_bind.as_ref());
        }
    });

    let buttons_map_unbind = buttons_map.clone();
    factory.connect_unbind(move |_, list_item| {
        if let Some(list_item) = list_item.downcast_ref::<ListItem>()
            && let Some(boxed) = list_item.item()
            && let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>()
        {
            let album = album_obj.borrow::<Arc<Album>>();
            buttons_map_unbind.borrow_mut().remove(&album.id);
        }
    });

    let subscription_handle =
        app_state.map(|state| spawn_state_subscription(state.clone(), buttons_map));

    let column = ColumnViewColumn::new(None::<&str>, Some(factory.upcast::<ListItemFactory>()));
    column.set_fixed_width(fixed_width);
    column.set_resizable(false);
    column_view.append_column(&column);

    subscription_handle
}

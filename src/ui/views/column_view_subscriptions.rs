//! Column view subscriptions for reactive updates.

use std::{cell::Cell, rc::Rc, sync::Arc};

use {
    libadwaita::{
        glib::{BoxedAnyObject, JoinHandle, MainContext},
        gtk::{
            MultiSelection,
            prelude::{Cast, ListModelExt, SelectionModelExt},
        },
    },
    tracing::debug,
};

use crate::{
    library::models::{Album, Artist},
    state::{
        app_state::{
            AppState,
            AppStateEvent::{
                CurrentTrackChanged, PlaybackStateChanged, QueueChanged, SelectionChanged,
                SettingsChanged,
            },
            LibraryTab::{Albums, Artists},
        },
        zoom_manager::ZoomEvent::ListZoomChanged,
    },
    ui::views::column_view_types::{
        ColumnListViewConfig,
        ColumnListViewType::{self, Albums as ViewAlbums, Artists as ViewArtists},
        SubscriptionHandles,
    },
};

/// Creates a subscription to zoom change events.
///
/// # Arguments
///
/// * `state` - Application state reference
///
/// # Returns
///
/// A join handle for the subscription task
#[must_use]
pub fn create_zoom_subscription(state: &Arc<AppState>) -> JoinHandle<()> {
    let state_clone = Arc::clone(state);
    MainContext::default().spawn_local(async move {
        let rx = state_clone.zoom_manager.subscribe();
        while let Ok(event) = rx.recv().await {
            if let ListZoomChanged(_) = &*event {
                debug!("ColumnListView: Zoom level changed, updating cover art dimensions");
            }
        }
    })
}

/// Creates a subscription to settings change events.
///
/// # Arguments
///
/// * `state` - Application state reference
/// * `view_type` - View type for filtering events
///
/// # Returns
///
/// A join handle for the subscription task
#[must_use]
pub fn create_settings_subscription(
    state: &Arc<AppState>,
    view_type: ColumnListViewType,
) -> JoinHandle<()> {
    let state_clone = Arc::clone(state);
    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            if let SettingsChanged {
                show_dr_values: _, ..
            } = &*event
                && view_type == ViewAlbums
            {
                debug!("ColumnListView: Settings changed, updating DR badge visibility");
            }
        }
    })
}

/// Creates a subscription to playback state events.
///
/// # Arguments
///
/// * `state` - Application state reference
///
/// # Returns
///
/// A join handle for the subscription task
#[must_use]
pub fn create_playback_subscription(state: &Arc<AppState>) -> JoinHandle<()> {
    let state_clone = Arc::clone(state);
    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            if matches!(
                &*event,
                CurrentTrackChanged(_) | QueueChanged(_) | PlaybackStateChanged(_)
            ) {
                debug!("ColumnListView: Playback state changed, updating play button icons");
            }
        }
    })
}

/// Creates a subscription to selection change events.
///
/// # Arguments
///
/// * `state` - Application state reference
/// * `config` - View configuration
/// * `selection_model` - GTK selection model
/// * `is_syncing` - Flag to prevent feedback loops
///
/// # Returns
///
/// A join handle for the subscription task
pub fn create_selection_subscription(
    state: &Arc<AppState>,
    config: &ColumnListViewConfig,
    selection_model: &MultiSelection,
    is_syncing: &Rc<Cell<bool>>,
) -> JoinHandle<()> {
    let state_clone = Arc::clone(state);
    let view_type = config.view_type.clone();
    let selection_model = selection_model.clone();
    let is_syncing = Rc::clone(is_syncing);

    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            if let SelectionChanged { tab, selected_ids } = event.as_ref() {
                let matches_tab = matches!(
                    (&view_type, tab),
                    (ViewAlbums, Albums) | (ViewArtists, Artists)
                );

                if matches_tab {
                    is_syncing.set(true);
                    let n_items = selection_model.n_items();
                    for i in 0..n_items {
                        if let Some(obj) = selection_model.item(i)
                            && let Ok(boxed) = obj.downcast::<BoxedAnyObject>()
                        {
                            let id = match view_type {
                                ViewAlbums => boxed.borrow::<Arc<Album>>().id,
                                ViewArtists => boxed.borrow::<Arc<Artist>>().id,
                            };

                            let should_be_selected = selected_ids.contains(&id);
                            if selection_model.is_selected(i) != should_be_selected {
                                if should_be_selected {
                                    selection_model.select_item(i, false);
                                } else {
                                    selection_model.unselect_item(i);
                                }
                            }
                        }
                    }
                    is_syncing.set(false);
                }
            }
        }
    })
}

/// Sets up subscriptions for reactive updates.
///
/// # Arguments
///
/// * `state` - Application state reference
/// * `config` - Configuration containing view type
/// * `selection_model` - GTK selection model
/// * `is_syncing` - Flag to prevent feedback loops
///
/// # Returns
///
/// Tuple of (`zoom_handle`, `settings_handle`, `playback_handle`, `selection_handle`)
pub fn setup_subscriptions(
    state: &Arc<AppState>,
    config: &ColumnListViewConfig,
    selection_model: &MultiSelection,
    is_syncing: &Rc<Cell<bool>>,
) -> SubscriptionHandles {
    let zoom_handle = Some(create_zoom_subscription(state));
    let settings_handle = Some(create_settings_subscription(
        state,
        config.view_type.clone(),
    ));
    let playback_handle = Some(create_playback_subscription(state));
    let selection_handle = Some(create_selection_subscription(
        state,
        config,
        selection_model,
        is_syncing,
    ));

    SubscriptionHandles {
        zoom_handle,
        settings_handle,
        playback_handle,
        selection_handle,
    }
}

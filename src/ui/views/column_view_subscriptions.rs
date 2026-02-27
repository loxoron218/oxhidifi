//! Column view subscriptions for reactive updates.

use std::sync::Arc;

use {
    libadwaita::glib::{JoinHandle, MainContext},
    tracing::debug,
};

use crate::{
    state::{
        AppState,
        ZoomEvent::ListZoomChanged,
        app_state::AppStateEvent::{
            CurrentTrackChanged, PlaybackStateChanged, QueueChanged, SettingsChanged,
        },
    },
    ui::views::column_view_types::{
        ColumnListViewConfig,
        ColumnListViewType::{self, Albums},
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
    let state_clone = state.clone();
    MainContext::default().spawn_local(async move {
        let rx = state_clone.zoom_manager.subscribe();
        while let Ok(event) = rx.recv().await {
            if let ListZoomChanged(_) = event {
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
    let state_clone = state.clone();
    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            if let SettingsChanged {
                show_dr_values: _, ..
            } = event
                && view_type == Albums
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
    let state_clone = state.clone();
    MainContext::default().spawn_local(async move {
        let rx = state_clone.subscribe();
        while let Ok(event) = rx.recv().await {
            if matches!(
                event,
                CurrentTrackChanged(_) | QueueChanged(_) | PlaybackStateChanged(_)
            ) {
                debug!("ColumnListView: Playback state changed, updating play button icons");
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
///
/// # Returns
///
/// Tuple of (`zoom_handle`, `settings_handle`, `playback_handle`)
#[must_use]
pub fn setup_subscriptions(
    state: &Arc<AppState>,
    config: &ColumnListViewConfig,
) -> SubscriptionHandles {
    let zoom_handle = Some(create_zoom_subscription(state));
    let settings_handle = Some(create_settings_subscription(
        state,
        config.view_type.clone(),
    ));
    let playback_handle = Some(create_playback_subscription(state));

    SubscriptionHandles {
        zoom_handle,
        settings_handle,
        playback_handle,
    }
}

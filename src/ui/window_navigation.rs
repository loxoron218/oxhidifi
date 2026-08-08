//! Window navigation events and active-tab persistence.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        glib::spawn_future_local,
        gtk::{Stack, Widget},
    },
    tracing::{info, warn},
};

use crate::{
    app::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail, Back},
    },
    storage::{
        database::SqliteStorage,
        settings::{
            ActiveTab,
            ActiveTab::{Albums, Artists},
        },
    },
    ui::{
        detail::{album::build_album_detail, artist::build_artist_detail},
        signal::ValueSignal,
    },
};

/// Save the active tab to storage asynchronously and notify subscribers.
pub fn persist_active_tab(
    storage: &Arc<SqliteStorage>,
    active_tab: &ValueSignal<ActiveTab>,
    name: &str,
) {
    let tab = if name == "artists" { Artists } else { Albums };
    let s = Arc::clone(storage);
    spawn_future_local(async move {
        if let Err(e) = s.set_active_tab(tab).await {
            warn!(error = %e, "Failed to save active tab");
        }
    });
    active_tab.send(tab);
}

/// Handle navigation events (album/artist detail, back navigation).
pub fn handle_navigation_event(
    nav_state: &Arc<AppState>,
    nav_content_area: &Stack,
    nav_tx: &Sender<NavigationEvent>,
    orig_stack: &Widget,
    event: NavigationEvent,
) {
    match event {
        AlbumDetail(album_id) => {
            info!(album_id, "Navigating to album detail",);
            if let Some(prev_detail) = nav_content_area.child_by_name("detail") {
                nav_content_area.remove(&prev_detail);
            }
            let detail = build_album_detail(nav_state, album_id, nav_tx);
            nav_content_area.add_named(&detail, Some("detail"));
            nav_content_area.set_visible_child(&detail);
        }
        ArtistDetail(artist_id) => {
            info!(artist_id, "Navigating to artist detail",);
            if let Some(prev_detail) = nav_content_area.child_by_name("detail") {
                nav_content_area.remove(&prev_detail);
            }
            let detail = build_artist_detail(nav_state, artist_id, nav_tx);
            nav_content_area.add_named(&detail, Some("detail"));
            nav_content_area.set_visible_child(&detail);
        }
        Back => {
            info!("Navigating back to library view");
            nav_content_area.set_visible_child(orig_stack);
            if let Some(prev_detail) = nav_content_area.child_by_name("detail") {
                nav_content_area.remove(&prev_detail);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::object::Cast,
            gtk::{self, Box, Orientation::Vertical, Stack, Widget, test},
        },
    };

    use crate::{
        app::{
            AppState,
            NavigationEvent::{AlbumDetail, Back},
        },
        storage::settings::ActiveTab::{Albums, Artists},
        ui::window_navigation::{handle_navigation_event, persist_active_tab},
    };

    #[test]
    fn persist_active_tab_maps_artists() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let rx = state.active_tab.subscribe();
        persist_active_tab(&state.storage, &state.active_tab, "artists");
        ensure!(state.active_tab.borrow() == Artists);
        ensure!(matches!(rx.try_recv(), Ok(Artists)));
        Ok(())
    }

    #[test]
    fn persist_active_tab_maps_albums() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let rx = state.active_tab.subscribe();
        state.active_tab.send(Artists);
        persist_active_tab(&state.storage, &state.active_tab, "albums");
        ensure!(state.active_tab.borrow() == Albums);
        ensure!(matches!(rx.try_recv(), Ok(Artists)));
        ensure!(matches!(rx.try_recv(), Ok(Albums)));
        Ok(())
    }

    #[test]
    fn handle_navigation_event_adds_and_removes_detail() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let content_area = Stack::new();
        let orig_stack = Box::new(Vertical, 0);
        content_area.add_named(&orig_stack, Some("library"));
        content_area.set_visible_child(&orig_stack);
        let nav_tx = state.navigation_tx.clone();
        let orig: Widget = orig_stack.upcast();

        handle_navigation_event(&state, &content_area, &nav_tx, &orig, AlbumDetail(1));
        ensure!(content_area.child_by_name("detail").is_some());

        handle_navigation_event(&state, &content_area, &nav_tx, &orig, Back);
        ensure!(content_area.child_by_name("detail").is_none());
        Ok(())
    }
}

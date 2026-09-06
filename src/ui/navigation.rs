//! Window navigation events and active-tab persistence.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{NavigationPage, NavigationView, glib::spawn_future_local},
    tracing::{info, warn},
};

use crate::{
    app::runtime::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail, Back},
    },
    storage::{
        active_tab::{
            ActiveTab,
            ActiveTab::{Albums, Artists},
        },
        database::SqliteStorage,
    },
    ui::{
        detail::{album_page::build_album_detail, artist_page::build_artist_detail},
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
///
/// Page stacks are managed via [`NavigationView`]: detail pages are pushed
/// as [`NavigationPage`]s with tag `detail`, and back navigation pops to the
/// `library` tag. This satisfies Constitution III which mandates
/// `AdwNavigationView` for push/pop stacks.
pub fn handle_navigation_event(
    nav_state: &Arc<AppState>,
    nav_view: &NavigationView,
    nav_tx: &Sender<NavigationEvent>,
    event: NavigationEvent,
) {
    match event {
        AlbumDetail(album_id) => {
            info!(album_id, "Navigating to album detail",);
            if let Some(prev) = nav_view.find_page("detail") {
                nav_view.remove(&prev);
            }
            let detail = build_album_detail(nav_state, album_id, nav_tx);
            let page = NavigationPage::builder()
                .child(&detail)
                .title("Album")
                .tag("detail")
                .build();
            nav_view.push(&page);
        }
        ArtistDetail(artist_id) => {
            info!(artist_id, "Navigating to artist detail",);
            if let Some(prev) = nav_view.find_page("detail") {
                nav_view.remove(&prev);
            }
            let detail = build_artist_detail(nav_state, artist_id, nav_tx);
            let page = NavigationPage::builder()
                .child(&detail)
                .title("Artist")
                .tag("detail")
                .build();
            nav_view.push(&page);
        }
        Back => {
            info!("Navigating back to library view");
            nav_view.pop_to_tag("library");
            if let Some(stale) = nav_view.find_page("detail") {
                nav_view.remove(&stale);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        async_channel::Sender,
        libadwaita::{
            NavigationPage, NavigationView,
            gtk::{self, Box, Orientation::Vertical, test},
        },
    };

    use crate::{
        app::runtime::{
            AppState,
            NavigationEvent::{self, AlbumDetail, Back},
        },
        storage::active_tab::ActiveTab::{Albums, Artists},
        ui::navigation::{handle_navigation_event, persist_active_tab},
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
        let nav_view = NavigationView::new();
        let nav_tx = add_library_page(&nav_view, &state);

        handle_navigation_event(&state, &nav_view, &nav_tx, AlbumDetail(1));
        ensure!(
            nav_view.find_page("detail").is_some(),
            "detail page must be pushed"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            nav_view.find_page("detail").is_none(),
            "detail page must be popped on Back"
        );
        ensure!(
            nav_view.find_page("library").is_some(),
            "library page must remain"
        );
        Ok(())
    }

    fn add_library_page(
        nav_view: &NavigationView,
        state: &Arc<AppState>,
    ) -> Sender<NavigationEvent> {
        let library = Box::builder().orientation(Vertical).spacing(0).build();
        let page = NavigationPage::builder()
            .child(&library)
            .title("Library")
            .tag("library")
            .build();
        nav_view.add(&page);
        state.navigation_tx.clone()
    }
}

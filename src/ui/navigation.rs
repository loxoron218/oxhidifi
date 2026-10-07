//! Window navigation events and active-tab persistence.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        NavigationPage, NavigationView,
        glib::{object::IsA, spawn_future_local},
        gtk::Widget,
    },
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
            ActiveTab::{Albums, Artists, Signal},
        },
        database::SqliteStorage,
    },
    ui::{
        detail::{album_page::build_album_detail, artist_page::build_artist_detail},
        signal_handlers::{UiHandles, ValueSignal},
    },
};

/// Save the active tab to storage asynchronously and notify subscribers.
///
/// Maps the visible stack child name (`albums`, `artists`, `signal`) to its
/// tab; unknown names fall back to the albums tab.
pub fn persist_active_tab(
    storage: &Arc<SqliteStorage>,
    active_tab: &ValueSignal<ActiveTab>,
    name: &str,
) {
    let tab = if name == "artists" {
        Artists
    } else if name == "signal" {
        Signal
    } else {
        Albums
    };
    let s = Arc::clone(storage);
    let mut handles = UiHandles::default();
    handles.retain_task(spawn_future_local(async move {
        if let Err(e) = s.set_active_tab(tab).await {
            warn!(error = %e, "Failed to save active tab");
        }
    }));
    active_tab.send(tab);
}

/// Check whether a detail page is currently visible.
///
/// Detail pages are pushed with tag `detail` (see [`handle_navigation_event`]).
/// While one is visible the library view-switch control (grid/column toggle
/// with its zoom popover) is a no-op — detail covers use fixed sizes — so
/// callers hide it and ignore `Ctrl+`/`Ctrl-` zoom.
///
/// This checks the *visible* page tag rather than stack presence: `Back`
/// pops to `library` before removing the stale `detail` page, so a
/// presence check would still report a detail during the pop notification
/// and the toggle would never come back.
///
/// # Arguments
///
/// * `nav_view` - Navigation view hosting the `library` and `detail` pages.
///
/// # Returns
///
/// `true` when the visible page is tagged `detail`, `false` otherwise.
#[must_use]
pub fn is_detail_visible(nav_view: &NavigationView) -> bool {
    nav_view.visible_page_tag().is_some_and(|tag| tag == "detail")
}

/// Build the `library` navigation page hosting the library view stack.
///
/// Centralizes the tag/title pairing so production code and tests never
/// drift apart (a `find_page("detail")`/`pop_to_tag("library")` mismatch
/// would silently break detail navigation and header visibility).
///
/// # Arguments
///
/// * `child` - Library content widget (usually the tab `ViewStack`).
///
/// # Returns
///
/// A `NavigationPage` tagged `library` titled `Library`.
#[must_use]
pub fn build_library_page(child: &impl IsA<Widget>) -> NavigationPage {
    NavigationPage::builder()
        .child(child)
        .title("Library")
        .tag("library")
        .build()
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
            info!(target = "library", "Navigating back to library view");
            _ = nav_view.pop_to_tag("library");
            if let Some(stale) = nav_view.find_page("detail") {
                nav_view.remove(&stale);
            }
        }
    }
}

/// Unit tests for navigation events, detail visibility, and test helpers.
#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        async_channel::Sender,
        libadwaita::{
            NavigationView,
            gtk::{self, Box, Orientation::Vertical, test},
        },
    };

    use crate::{
        app::runtime::{
            AppState,
            NavigationEvent::{self, AlbumDetail, Back},
        },
        storage::active_tab::ActiveTab::{Albums, Artists, Signal},
        ui::navigation::{
            build_library_page, handle_navigation_event, is_detail_visible, persist_active_tab,
        },
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
    fn persist_active_tab_maps_signal() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let rx = state.active_tab.subscribe();
        persist_active_tab(&state.storage, &state.active_tab, "signal");
        ensure!(state.active_tab.borrow() == Signal);
        ensure!(matches!(rx.try_recv(), Ok(Signal)));
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
        ensure!(
            is_detail_visible(&nav_view),
            "detail helper must report a pushed detail page"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            nav_view.find_page("detail").is_none(),
            "detail page must be popped on Back"
        );
        ensure!(
            !is_detail_visible(&nav_view),
            "detail helper must report no detail after Back"
        );
        ensure!(
            nav_view.find_page("library").is_some(),
            "library page must remain"
        );
        Ok(())
    }

    #[test]
    fn is_detail_visible_false_on_library_only() -> Result<()> {
        let nav_view = NavigationView::new();
        let library = Box::builder().orientation(Vertical).spacing(0).build();
        nav_view.add(&build_library_page(&library));
        ensure!(
            !is_detail_visible(&nav_view),
            "library-only stack must not report a detail page"
        );
        Ok(())
    }

    /// Add the `library` page to a navigation view for tests.
    ///
    /// Shared fixture so navigation and header tests build the same page
    /// instead of duplicating the tag/title pairing.
    ///
    /// # Arguments
    ///
    /// * `nav_view` - Navigation view receiving the `library` page.
    /// * `state` - Application state owning the navigation channel.
    ///
    /// # Returns
    ///
    /// The navigation event sender for driving detail/`Back` navigation.
    pub fn add_library_page(
        nav_view: &NavigationView,
        state: &Arc<AppState>,
    ) -> Sender<NavigationEvent> {
        let library = Box::builder().orientation(Vertical).spacing(0).build();
        nav_view.add(&build_library_page(&library));
        state.navigation_tx.clone()
    }
}

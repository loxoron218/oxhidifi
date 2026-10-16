//! Window navigation events and active-tab persistence.

use std::sync::Arc;

use {
    libadwaita::{
        NavigationPage, NavigationView,
        glib::{object::IsA, spawn_future_local},
        gtk::{Widget, accessible::Property::Label},
        prelude::AccessibleExtManual,
    },
    tracing::warn,
};

use crate::{
    storage::{active_tab::ActiveTab, database::SqliteStorage},
    ui::{
        nav_tags::is_detail_tag,
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
    let tab = ActiveTab::from_stack_name(name);
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
/// Detail pages are pushed with per-entity tags (see
/// [`handle_navigation_event`]). While one is visible the library
/// view-switch control (grid/column toggle with its zoom popover) is a no-op
/// — detail covers use fixed sizes — so callers hide it and ignore
/// `Ctrl+`/`Ctrl-` zoom.
///
/// This checks the *visible* page tag rather than stack presence: `Back`
/// pops a single page, so a presence check would still report a detail during
/// the pop notification and the toggle would never come back.
///
/// # Arguments
///
/// * `nav_view` - Navigation view hosting the `library` and detail pages.
///
/// # Returns
///
/// `true` when the visible page is tagged as detail, `false` otherwise.
#[must_use]
pub fn is_detail_visible(nav_view: &NavigationView) -> bool {
    nav_view
        .visible_page_tag()
        .is_some_and(|tag| is_detail_tag(tag.as_str()))
}

/// Build the `library` navigation page hosting the library view stack.
///
/// Centralizes the tag/title pairing so production code and tests never
/// drift apart (a pushed-tag/`pop` mismatch would silently break detail
/// navigation and header visibility).
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
    let page = NavigationPage::builder()
        .child(child)
        .title("Library")
        .tag("library")
        .css_classes(["library-page"])
        .tooltip_text("Library content")
        .build();
    page.update_property(&[Label("Library")]);
    page
}

/// Unit tests for navigation events, detail visibility, and test helpers.
#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        async_channel::Sender,
        libadwaita::{
            NavigationView,
            gtk::{self, Box, Orientation::Vertical, accessible::Property::Label, test},
            prelude::{AccessibleExtManual, WidgetExt},
        },
    };

    use crate::{
        app::runtime::{
            AppState,
            NavigationEvent::{self, AlbumDetail, ArtistDetail, Back},
        },
        storage::active_tab::ActiveTab::{Albums, Artists, Signal},
        ui::{
            detail_nav::handle_navigation_event,
            nav_tags::{detail_album_tag, detail_artist_tag},
            navigation::{build_library_page, is_detail_visible, persist_active_tab},
        },
    };

    fn test_nav_view() -> Result<(Arc<AppState>, NavigationView, Sender<NavigationEvent>)> {
        let state = Arc::new(AppState::mock()?);
        let nav_view = NavigationView::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test nav view")
            .build();
        nav_view.update_property(&[Label("Test nav view")]);
        let nav_tx = add_library_page(&nav_view, &state);
        Ok((state, nav_view, nav_tx))
    }

    #[test]
    fn navigation_pages_stay_in_focus_chain() -> Result<()> {
        let (state, nav_view, nav_tx) = test_nav_view()?;
        handle_navigation_event(&state, &nav_view, &nav_tx, AlbumDetail(1));
        let album_tag = detail_album_tag(1);
        ensure!(
            nav_view.can_focus(),
            "navigation view must stay focusable so keyboard focus (and Escape-to-back) reaches \
             pushed detail pages"
        );
        for tag in ["library", album_tag.as_str()] {
            let Some(page) = nav_view.find_page(tag) else {
                bail!("page {tag} must exist");
            };
            ensure!(
                page.can_focus(),
                "page {tag} must stay focusable so keyboard focus (and Escape-to-back) reaches \
                 pushed detail content"
            );
        }
        Ok(())
    }

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
        let (state, nav_view, nav_tx) = test_nav_view()?;

        handle_navigation_event(&state, &nav_view, &nav_tx, AlbumDetail(1));
        ensure!(
            nav_view.find_page(&detail_album_tag(1)).is_some(),
            "detail page must be pushed"
        );
        ensure!(
            is_detail_visible(&nav_view),
            "detail helper must report a pushed detail page"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            !is_detail_visible(&nav_view),
            "detail helper must report no detail after Back"
        );
        ensure!(
            nav_view
                .visible_page_tag()
                .is_some_and(|tag| tag == "library"),
            "Back must return to the library page"
        );
        ensure!(
            nav_view.find_page("library").is_some(),
            "library page must remain"
        );
        Ok(())
    }

    #[test]
    fn artist_to_album_drill_down_stacks_without_duplicate_tag() -> Result<()> {
        let (state, nav_view, nav_tx) = test_nav_view()?;

        handle_navigation_event(&state, &nav_view, &nav_tx, ArtistDetail(3));
        handle_navigation_event(&state, &nav_view, &nav_tx, AlbumDetail(4));
        ensure!(
            nav_view.find_page(&detail_artist_tag(3)).is_some(),
            "artist page must remain stacked"
        );
        ensure!(
            nav_view.find_page(&detail_album_tag(4)).is_some(),
            "album page must push on top of the artist page"
        );
        ensure!(
            nav_view
                .visible_page_tag()
                .is_some_and(|tag| tag == detail_album_tag(4)),
            "album detail must become visible"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, AlbumDetail(4));
        ensure!(
            nav_view
                .visible_page_tag()
                .is_some_and(|tag| tag == detail_album_tag(4)),
            "reopening the visible album must be a no-op"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            nav_view
                .visible_page_tag()
                .is_some_and(|tag| tag == detail_artist_tag(3)),
            "first Back must return to the artist page"
        );
        ensure!(
            is_detail_visible(&nav_view),
            "artist page must still count as detail"
        );

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            !is_detail_visible(&nav_view),
            "second Back must clear the detail stack"
        );
        ensure!(
            nav_view
                .visible_page_tag()
                .is_some_and(|tag| tag == "library"),
            "second Back must return to the library page"
        );
        Ok(())
    }

    #[test]
    fn back_on_library_only_is_noop() -> Result<()> {
        let (state, nav_view, nav_tx) = test_nav_view()?;

        handle_navigation_event(&state, &nav_view, &nav_tx, Back);
        ensure!(
            nav_view
                .visible_page_tag()
                .is_some_and(|tag| tag == "library"),
            "Back on library must stay on library"
        );
        ensure!(
            !is_detail_visible(&nav_view),
            "library must not count as detail"
        );
        Ok(())
    }

    #[test]
    fn is_detail_visible_false_on_library_only() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let nav_view = NavigationView::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test nav detail")
            .build();
        nav_view.update_property(&[Label("Test nav detail")]);
        drop(add_library_page(&nav_view, &state));
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
        let library = Box::builder()
            .orientation(Vertical)
            .spacing(0)
            .css_classes(["test-fixture"])
            .tooltip_text("Test lib box")
            .build();
        library.update_property(&[Label("Test lib box")]);
        nav_view.add(&build_library_page(&library));
        state.navigation_tx.clone()
    }
}

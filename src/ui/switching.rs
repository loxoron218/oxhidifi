//! Tab-switch reconciliation for the library view stack.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    async_channel::Sender,
    libadwaita::{NavigationView, ViewStack, gtk::Stack},
    tracing::{error, warn},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::Back},
    storage::{
        active_tab::ActiveTab::{self, Albums, Artists, Signal},
        view_mode::ViewMode::{Column, Grid},
    },
    ui::{
        gallery::{
            album_grid::lazy_build_album_mode, artist_build::lazy_build_artist_mode,
            narrow_flag::NarrowState,
        },
        navigation::{is_detail_visible, persist_active_tab},
    },
};

/// Show the destination tab and reconcile its grid with the current mode.
///
/// Hidden tabs are no longer rebuilt on view-mode changes (only the active
/// tab is), so switching here lazily builds the current mode child when it
/// is missing. When the mode child already exists, only a pending sort/zoom
/// change (`dirty`) triggers a rebuild.
///
/// The caller guarantees freshness: the library signal task drops queued
/// values superseded by a newer model, so a stale request never reaches this
/// function. As a final guard the already-visible child is left untouched,
/// keeping redundant notifications from re-entering the model.
pub fn handle_tab_switch(
    stack: &ViewStack,
    state: &Arc<AppState>,
    tab: ActiveTab,
    album_stack: &Stack,
    artist_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    let already_visible = stack
        .visible_child_name()
        .is_some_and(|current| current.as_str() == tab.stack_name());
    if !already_visible {
        stack.set_visible_child_name(tab.stack_name());
    }
    let mode = state.view_mode.borrow();
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    let (mode_stack, dirty, rebuild_tx) = match tab {
        Albums => (album_stack, &state.album_grid.dirty, &state.albums_sort_tx),
        Artists => (
            artist_stack,
            &state.artist_grid.dirty,
            &state.artists_sort_tx,
        ),
        Signal => return,
    };
    let was_dirty = dirty.swap(false, Relaxed);
    if mode_stack.child_by_name(child_name).is_some() {
        if was_dirty {
            signal_grid_rebuild(rebuild_tx);
        }
        return;
    }
    match tab {
        Albums => lazy_build_album_mode(state, album_stack, narrow_state, mode),
        Artists => lazy_build_artist_mode(state, artist_stack, narrow_state, mode),
        Signal => {}
    }
}

/// Signal a library grid to rebuild after a tab switch so it reflects
/// sort or zoom changes that arrived while the tab was hidden.
fn signal_grid_rebuild(tx: &Sender<()>) {
    if let Err(e) = tx.try_send(()) {
        warn!(error = %e, "Failed to signal grid rebuild after tab switch");
    }
}

/// Track tab switches and report detail-page exits.
///
/// Persists the active tab whenever the visible stack child changes and
/// sends a `Back` navigation event when a detail page closes.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handle.
/// * `stack` - Tab stack whose visible child is tracked.
/// * `nav_view` - Navigation view checked for detail-page presence.
pub fn wire_tab_tracking(state: &Arc<AppState>, stack: &ViewStack, nav_view: &NavigationView) {
    let tab_nav_tx = state.navigation_tx.clone();
    let tab_nav_view = nav_view.clone();
    let tab_stack = stack.clone();
    let tab_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(stack.connect_visible_child_notify(move |_| {
            if !is_detail_visible(&tab_nav_view)
                && let Some(name) = tab_stack.visible_child_name()
            {
                persist_active_tab(&tab_state, name.as_str());
            }
            if is_detail_visible(&tab_nav_view)
                && let Err(err) = tab_nav_tx.try_send(Back)
            {
                error!(error = %err, "Failed to send Back navigation event");
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering::Relaxed},
    };

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ViewStack,
            gtk::{self, Label, Stack, accessible::Property::Label as PropertyLabel, test},
            prelude::AccessibleExtManual,
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::active_tab::ActiveTab::{Albums, Artists, Signal},
        ui::{gallery::narrow_flag::NarrowState, switching::handle_tab_switch},
    };

    fn add_page(stack: &ViewStack, name: &str) {
        let label = Label::builder()
            .label(name)
            .css_classes(["test-fixture"])
            .tooltip_text("Test page")
            .build();
        label.update_property(&[PropertyLabel("Test page")]);
        drop(stack.add_named(&label, Some(name)));
    }

    fn add_stack_page(stack: &Stack, name: &str) {
        let label = Label::builder()
            .label(name)
            .css_classes(["test-fixture"])
            .tooltip_text("Test stack page")
            .build();
        label.update_property(&[PropertyLabel("Test stack page")]);
        drop(stack.add_named(&label, Some(name)));
    }

    fn test_stacks() -> (ViewStack, Stack, Stack) {
        let view_stack = ViewStack::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test view stack")
            .build();
        view_stack.update_property(&[PropertyLabel("Test view stack")]);
        add_page(&view_stack, "albums");
        add_page(&view_stack, "artists");
        add_page(&view_stack, "signal");
        let album_stack = Stack::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test album stack")
            .build();
        album_stack.update_property(&[PropertyLabel("Test album stack")]);
        add_stack_page(&album_stack, "grid");
        add_stack_page(&album_stack, "column");
        let artist_stack = Stack::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test artist stack")
            .build();
        artist_stack.update_property(&[PropertyLabel("Test artist stack")]);
        add_stack_page(&artist_stack, "grid");
        add_stack_page(&artist_stack, "column");
        (view_stack, album_stack, artist_stack)
    }

    #[test]
    fn tab_switch_shows_matching_page() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let narrow = NarrowState::new_shared();
        let (view_stack, album_stack, artist_stack) = test_stacks();
        handle_tab_switch(
            &view_stack,
            &state,
            Albums,
            &album_stack,
            &artist_stack,
            &narrow,
        );
        ensure!(
            view_stack
                .visible_child_name()
                .is_some_and(|n| n == "albums"),
            "albums tab must become visible"
        );
        handle_tab_switch(
            &view_stack,
            &state,
            Artists,
            &album_stack,
            &artist_stack,
            &narrow,
        );
        ensure!(
            view_stack
                .visible_child_name()
                .is_some_and(|n| n == "artists"),
            "artists tab must become visible"
        );
        handle_tab_switch(
            &view_stack,
            &state,
            Signal,
            &album_stack,
            &artist_stack,
            &narrow,
        );
        ensure!(
            view_stack
                .visible_child_name()
                .is_some_and(|n| n == "signal"),
            "signal tab must become visible without touching the grids"
        );
        Ok(())
    }

    #[test]
    fn redundant_tab_switch_skips_visible_child_set() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let narrow = NarrowState::new_shared();
        let (view_stack, album_stack, artist_stack) = test_stacks();
        view_stack.set_visible_child_name("artists");
        let notifications = Arc::new(AtomicU32::new(0));
        let counted = Arc::clone(&notifications);
        state
            .handles
            .lock()
            .retain_signal(view_stack.connect_visible_child_notify(move |_| {
                _ = counted.fetch_add(1, Relaxed);
            }));
        state.artist_grid.dirty.store(true, Relaxed);
        handle_tab_switch(
            &view_stack,
            &state,
            Artists,
            &album_stack,
            &artist_stack,
            &narrow,
        );
        ensure!(
            notifications.load(Relaxed) == 0,
            "redundant switch must not re-emit visible-child-notify"
        );
        ensure!(
            view_stack
                .visible_child_name()
                .is_some_and(|n| n == "artists"),
            "redundant switch must keep the artists tab visible"
        );
        ensure!(
            state.artists_sort_rx.try_recv().is_ok(),
            "redundant switch must still rebuild a dirty grid"
        );
        Ok(())
    }
}

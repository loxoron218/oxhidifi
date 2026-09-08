//! Tab-switch reconciliation for the library view stack.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    async_channel::Sender,
    libadwaita::{NavigationView, ViewStack, gtk::Stack},
    tracing::{error, info, warn},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::Back},
    storage::{
        active_tab::ActiveTab::{self, Albums, Artists},
        view_mode::ViewMode::{Column, Grid},
    },
    ui::{
        gallery::{
            album_grid::lazy_build_album_mode, artist_grid::lazy_build_artist_mode,
            narrow_flag::NarrowState,
        },
        navigation::persist_active_tab,
    },
};

/// Show the destination tab and reconcile its grid with the current mode.
///
/// Hidden tabs are no longer rebuilt on view-mode changes (only the active
/// tab is), so switching here lazily builds the current mode child when it
/// is missing. When the mode child already exists, only a pending sort/zoom
/// change (`dirty`) triggers a rebuild.
pub fn handle_tab_switch(
    stack: &ViewStack,
    state: &Arc<AppState>,
    tab: ActiveTab,
    album_stack: &Stack,
    artist_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    stack.set_visible_child_name(match tab {
        Albums => "albums",
        Artists => "artists",
    });
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
        Artists => lazy_build_artist_mode(state, artist_stack, mode),
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
    let tab_storage = Arc::clone(&state.storage);
    let tab_active_tab = state.active_tab.clone();
    state
        .handles
        .lock()
        .retain_signal(stack.connect_visible_child_notify(move |_| {
            if tab_nav_view.find_page("detail").is_none()
                && let Some(name) = tab_stack.visible_child_name()
            {
                info!(tab_name = name.as_str(), "Tab switched",);
                persist_active_tab(&tab_storage, &tab_active_tab, name.as_str());
            }
            let is_on_detail = tab_nav_view.find_page("detail").is_some();
            if is_on_detail && let Err(err) = tab_nav_tx.try_send(Back) {
                error!(error = %err, "Failed to send Back navigation event");
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ViewStack,
            gtk::{self, Label, Stack, test},
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::active_tab::ActiveTab,
        ui::{gallery::narrow_flag::NarrowState, switching::handle_tab_switch},
    };

    fn add_page(stack: &ViewStack, name: &str) {
        drop(stack.add_named(&Label::new(Some(name)), Some(name)));
    }

    fn add_stack_page(stack: &Stack, name: &str) {
        drop(stack.add_named(&Label::new(Some(name)), Some(name)));
    }

    #[test]
    fn tab_switch_shows_matching_page() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let narrow = NarrowState::new_shared();
        let view_stack = ViewStack::new();
        add_page(&view_stack, "albums");
        add_page(&view_stack, "artists");
        let album_stack = Stack::new();
        add_stack_page(&album_stack, "grid");
        add_stack_page(&album_stack, "column");
        let artist_stack = Stack::new();
        add_stack_page(&artist_stack, "grid");
        add_stack_page(&artist_stack, "column");
        handle_tab_switch(
            &view_stack,
            &state,
            ActiveTab::Albums,
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
            ActiveTab::Artists,
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
        Ok(())
    }
}

//! Tab-switch reconciliation for the library view stack.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    async_channel::Sender,
    libadwaita::{ViewStack, gtk::Stack},
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::{
        active_tab::ActiveTab::{self, Albums, Artists},
        view_mode::ViewMode::{Column, Grid},
    },
    ui::gallery::{
        album_grid::lazy_build_album_mode, artist_grid::lazy_build_artist_mode,
        narrow_flag::NarrowState,
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

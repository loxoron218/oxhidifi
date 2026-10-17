//! Library view stack: pages, tab-signal wiring, and navigation page.
//!
//! Owns the Albums/Artists/Signal `ViewStack`, wires the `active_tab`
//! fan-out to it, and assembles the `library` navigation page hosting the
//! stack. Tab requests apply through model transitions; the signal task drops
//! superseded backlog so bursts settle on the latest tab instead of echoing.

use std::sync::Arc;

use libadwaita::{
    NavigationPage, ViewStack,
    glib::{object::IsA, spawn_future_local},
    gtk::{Stack, Widget, accessible::Property::Label},
    prelude::{AccessibleExtManual, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    storage::active_tab::ActiveTab::{self, Albums, Artists, Signal},
    ui::{
        gallery::{
            album_grid::build_album_grid, artist_grid::build_artist_grid, narrow_flag::NarrowState,
        },
        pane_modes::switch_mode_for_active_tab,
        signal_view::{signal_tab::SignalTab, signal_tab_build::build_signal_page},
        switching::handle_tab_switch,
    },
};

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

/// Build the library `ViewStack` with album, artist, and signal pages.
pub fn build_library_stack(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
) -> (ViewStack, Stack, Stack, SignalTab) {
    let stack = ViewStack::builder()
        .css_classes(["library-stack"])
        .tooltip_text("Library views")
        .build();
    stack.update_property(&[Label("Library views")]);
    stack.set_vexpand(true);
    let album_grid = build_album_grid(state, narrow_state);
    drop(stack.add_titled_with_icon(
        &album_grid.mode_stack,
        Some("albums"),
        "Albums",
        "view-grid-symbolic",
    ));
    let artist_grid = build_artist_grid(state, narrow_state);
    drop(stack.add_titled_with_icon(
        &artist_grid.mode_stack,
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    ));
    let signal_tab = build_signal_page();
    drop(stack.add_titled_with_icon(
        signal_tab.widget(),
        Some("signal"),
        "Signal",
        "audio-x-generic-symbolic",
    ));
    match state.storage.get_active_tab() {
        Artists => stack.set_visible_child_name("artists"),
        Signal => stack.set_visible_child_name("signal"),
        Albums => {}
    }
    (
        stack,
        album_grid.mode_stack,
        artist_grid.mode_stack,
        signal_tab,
    )
}

/// Apply one queued tab request unless a newer model value superseded it.
///
/// Stale backlog can neither move the widget backward nor echo a new model
/// transition: only the latest model value reaches
/// [`handle_tab_switch`].
///
/// # Arguments
///
/// * `state` - Application state owning the authoritative tab value.
/// * `stack` - Tab stack receiving the visible child.
/// * `tab` - Queued tab request.
/// * `album_stack` - Mode stack for the albums tab.
/// * `artist_stack` - Mode stack for the artists tab.
/// * `narrow_state` - Shared narrow-mode flag for grid layout.
fn apply_tab_if_current(
    state: &Arc<AppState>,
    stack: &ViewStack,
    tab: ActiveTab,
    album_stack: &Stack,
    artist_stack: &Stack,
    narrow_state: &Arc<NarrowState>,
) {
    if state.active_tab.borrow() != tab {
        return;
    }
    handle_tab_switch(stack, state, tab, album_stack, artist_stack, narrow_state);
}

/// Wire tab and view-mode signals for the library stack.
///
/// The tab task applies only the latest model value, dropping superseded
/// backlog. Without this, rapid bursts (for example held `Ctrl+Tab`)
/// recirculate through the stack notify / model echo and never settle.
pub fn wire_library_signals(
    state: &Arc<AppState>,
    stack: &ViewStack,
    album_stack: Stack,
    artist_stack: Stack,
    narrow_state: Arc<NarrowState>,
) {
    let tab_rx = state.active_tab.subscribe();
    let s1 = stack.clone();
    let st1 = Arc::clone(state);
    let a1 = album_stack.clone();
    let r1 = artist_stack.clone();
    let n1 = Arc::clone(&narrow_state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            while let Ok(tab) = tab_rx.recv().await {
                apply_tab_if_current(&st1, &s1, tab, &a1, &r1, &n1);
            }
        }));
    let s2 = Arc::clone(state);
    let a2 = album_stack;
    let r2 = artist_stack;
    let n2 = narrow_state;
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let rx = s2.view_mode.subscribe();
            while let Ok(m) = rx.recv().await {
                switch_mode_for_active_tab(&s2, m, &a2, &r2, &n2);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::MainContext,
            gtk::{self, test},
        },
        parking_lot::Mutex,
    };

    use crate::{
        app::{preview_state::pump_in_test_runtime, runtime::AppState},
        storage::active_tab::ActiveTab::{Artists, Signal},
        ui::{
            gallery::narrow_flag::NarrowState,
            library_stack::{build_library_stack, wire_library_signals},
            navigation::request_active_tab,
        },
    };

    #[test]
    fn rapid_tab_burst_settles_on_latest() -> Result<()> {
        pump_in_test_runtime(|| {
            let state = Arc::new(AppState::mock()?);
            let narrow_state = NarrowState::new_shared();
            let (stack, album_stack, artist_stack, _) = build_library_stack(&state, &narrow_state);
            wire_library_signals(
                &state,
                &stack,
                album_stack,
                artist_stack,
                Arc::clone(&narrow_state),
            );
            stack.set_visible_child_name("albums");
            let history = Arc::new(Mutex::new(Vec::new()));
            let recorded = Arc::clone(&history);
            let named = stack.clone();
            state
                .handles
                .lock()
                .retain_signal(stack.connect_visible_child_notify(move |_| {
                    if let Some(name) = named.visible_child_name() {
                        recorded.lock().push(name.to_string());
                    }
                }));
            request_active_tab(&state, Artists);
            request_active_tab(&state, Signal);
            for _ in 0..1000 {
                if !MainContext::default().iteration(false) {
                    break;
                }
            }
            let settled = history.lock().clone();
            ensure!(
                stack
                    .visible_child_name()
                    .is_some_and(|name| name == "signal"),
                "burst must settle on the latest requested tab"
            );
            ensure!(
                settled.len() <= 1,
                "burst must apply at most one visible switch, got {settled:?}"
            );
            for _ in 0..1000 {
                if !MainContext::default().iteration(false) {
                    break;
                }
            }
            ensure!(
                *history.lock() == settled,
                "settled burst must not keep switching"
            );
            Ok(())
        })?
    }
}

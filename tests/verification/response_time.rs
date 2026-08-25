//! UI response verification (T052c) per SC-005.
//!
//! Drives the GTK main thread to time library navigation: Album ↔ Artist tab
//! switches, grid/column view toggle, and album detail navigation. Each
//! operation must complete in under 100 ms, recorded via the `UiResponse`
//! metrics collector.
//!
//! Gated behind the `verification-tests` feature. Run with:
//!
//! ```text
//! cargo test --features verification-tests --test response_time
//! ```

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use {
    anyhow::{Context, Result, ensure},
    async_channel::unbounded,
    tokio::runtime::Runtime,
};

use oxhidifi::{
    app::runtime::{AppChannels, AppState, build_broadcast_channels},
    library::scanner::FsScanner,
    metrics::UiResponse,
    playback::engine::PlaybackEngine,
    storage::{active_tab::ActiveTab::Albums, database::SqliteStorage, view_mode::ViewMode::Grid},
    threading::ThreadManager,
};

/// SC-005 UI response threshold in milliseconds.
const UI_RESPONSE_THRESHOLD_MS: f64 = 100.0;

/// Build a mock `AppState` backed by an in-memory SQLite database.
///
/// Mirrors `AppState::mock()` (which is `#[cfg(test)]`-only and thus not
/// available to integration tests) by constructing the same component graph.
fn mock_state() -> Result<Arc<AppState>> {
    let rt = Runtime::new().context("failed to create tokio runtime")?;
    let storage = Arc::new(
        rt.block_on(SqliteStorage::connect_with_settings_path(
            Path::new(":memory:"),
            Path::new("/tmp/oxhidifi-ui-response-settings.json"),
        ))
        .context("failed to create in-memory storage")?,
    );

    let (scan_event_tx, scan_event_rx) = unbounded();
    let (toast_tx, toast_rx) = unbounded();
    let (navigation_tx, navigation_rx) = unbounded();
    let channels = AppChannels {
        scan_event_tx,
        scan_event_rx,
        toast_tx,
        toast_rx,
        navigation_tx,
        navigation_rx,
    };
    let broadcast = build_broadcast_channels(Grid, Albums);

    let scanner_storage = Arc::clone(&storage);
    let (scanner_tx, _) = unbounded();
    let state = AppState::new(
        Arc::new(PlaybackEngine::new()),
        storage,
        Arc::new(FsScanner::new(scanner_storage, scanner_tx, 4)),
        channels,
        broadcast,
        Arc::new(ThreadManager::new()),
    );
    Ok(Arc::new(state))
}

/// Time a single UI operation, record it via `UiResponse`, and assert it is
/// under the SC-005 threshold.
fn time_operation(action: &'static str, f: impl FnOnce()) -> Result<()> {
    let start = Instant::now();
    f();
    let elapsed = start.elapsed();
    UiResponse::record(action, elapsed);
    ensure!(
        elapsed < Duration::from_secs_f64(UI_RESPONSE_THRESHOLD_MS / 1000.0),
        "SC-005: '{action}' took {:.2} ms (threshold {UI_RESPONSE_THRESHOLD_MS} ms)",
        elapsed.as_secs_f64() * 1000.0
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Context, Result},
        libadwaita::{
            ViewStack,
            glib::object::Cast,
            gtk::{Box, Orientation::Vertical, Stack, Widget, init},
        },
    };

    use oxhidifi::{
        app::runtime::NavigationEvent::{AlbumDetail, Back},
        storage::{
            active_tab::ActiveTab::{Albums, Artists},
            view_mode::ViewMode::{Column, Grid},
        },
        ui::{
            gallery::narrow_flag::NarrowState, navigation::handle_navigation_event,
            switching::handle_tab_switch,
        },
    };

    use crate::{mock_state, time_operation};

    #[test]
    fn ui_navigation_response_under_threshold() -> Result<()> {
        init().context("failed to initialize GTK")?;

        let state = mock_state()?;

        let stack = ViewStack::new();
        let album_stack = Stack::new();
        let artist_stack = Stack::new();
        let narrow_state = NarrowState::new_shared();

        for (tab, m) in [
            (Albums, Grid),
            (Albums, Column),
            (Artists, Grid),
            (Artists, Column),
        ] {
            state.view_mode.send(m);
            handle_tab_switch(
                &stack,
                &state,
                tab,
                &album_stack,
                &artist_stack,
                &narrow_state,
            );
        }
        state.view_mode.send(Grid);

        time_operation("tab_albums_to_artists", || {
            handle_tab_switch(
                &stack,
                &state,
                Artists,
                &album_stack,
                &artist_stack,
                &narrow_state,
            );
        })?;
        time_operation("tab_artists_to_albums", || {
            handle_tab_switch(
                &stack,
                &state,
                Albums,
                &album_stack,
                &artist_stack,
                &narrow_state,
            );
        })?;

        time_operation("toggle_grid_to_column", || {
            state.view_mode.send(Column);
            handle_tab_switch(
                &stack,
                &state,
                Albums,
                &album_stack,
                &artist_stack,
                &narrow_state,
            );
        })?;
        time_operation("toggle_column_to_grid", || {
            state.view_mode.send(Grid);
            handle_tab_switch(
                &stack,
                &state,
                Albums,
                &album_stack,
                &artist_stack,
                &narrow_state,
            );
        })?;

        let content_area = Stack::new();
        let orig_stack = Box::new(Vertical, 0);
        content_area.add_named(&orig_stack, Some("library"));
        content_area.set_visible_child(&orig_stack);
        let nav_tx = state.navigation_tx.clone();
        let orig: Widget = orig_stack.upcast();

        time_operation("open_album_detail", || {
            handle_navigation_event(&state, &content_area, &nav_tx, &orig, AlbumDetail(1));
        })?;
        time_operation("back_to_library", || {
            handle_navigation_event(&state, &content_area, &nav_tx, &orig, Back);
        })?;

        Ok(())
    }
}

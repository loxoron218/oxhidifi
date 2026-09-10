//! Sort/zoom coalescing loop that debounces grid rebuilds.

use std::time::Duration;

use {async_channel::Receiver, tokio::select};

use crate::storage::{
    active_tab::ActiveTab,
    view_mode::ViewMode::{self, Grid},
};

/// Debounce window for coalescing rapid sort/zoom changes before a grid rebuild.
pub const SORT_ZOOM_DEBOUNCE: Duration = Duration::from_millis(200);

/// What a grid's sort/zoom rebuild handler should do after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuildAction {
    /// Resize the live grid cards in place (zoom-only change on a ready grid).
    /// Falls back to a full rebuild if the in-place resize cannot run.
    Resize,
    /// Defer the rebuild until the tab is shown again (mark the grid dirty).
    DeferDirty,
    /// Rebuild the current view mode from the in-memory cache.
    Rebuild,
}

/// Event dispatched from the coalescing loop to the widget-side consumer.
///
/// The loop runs as a `Send` future, so the preview/rebuild closures it holds
/// can only carry `Send` data.  Widget-touching work is performed by a
/// separate local consumer that receives these events in FIFO order.
#[derive(Debug, Clone, Copy)]
pub enum SortZoomEvent {
    /// Run the immediate zoom preview before the debounce window elapses.
    Preview,
    /// Run the debounced rebuild with the coalesced `(sort_fired, zoom_fired)` flags.
    Rebuild(bool, bool),
}

/// Decide how a library grid should react to a sort/zoom change.
///
/// Shared by the album and artist grids so their rebuild behavior can never
/// drift apart. The caller passes `tab` explicitly rather than hardcoding it
/// per view, and attempts the in-place resize itself when [`RebuildAction::Resize`]
/// is returned.
#[must_use]
pub fn decide_rebuild(
    active_tab: ActiveTab,
    tab: ActiveTab,
    view_mode: ViewMode,
    sort_fired: bool,
    zoom_fired: bool,
    ready: bool,
) -> RebuildAction {
    if active_tab != tab {
        return RebuildAction::DeferDirty;
    }
    if view_mode == Grid && zoom_fired && !sort_fired && ready {
        RebuildAction::Resize
    } else {
        RebuildAction::Rebuild
    }
}

/// Debounce and rebuild loop for a single library grid.
///
/// Coalesces sort/zoom changes within a debounce window (driven by the
/// `debounce` async callback), tracking which channel fired so the rebuild
/// handler can choose between a cheap in-place resize (zoom only) and a full
/// rebuild (sort involved). A zoom-only change also runs `preview_zoom`
/// immediately on the first wake, so the live cards resize before the
/// debounce window elapses instead of after. Exits when either channel
/// closes.
///
/// Uses `async_channel` receivers (not `tokio::sync`) so the futures wake
/// reliably on the `GLib` main context, and the `debounce` callback is a
/// glib-native timer in production — see `spawn_listen_sort_zoom`.
pub async fn listen_sort_zoom_loop(
    sort_rx: Receiver<()>,
    zoom_rx: Receiver<()>,
    debounce: impl AsyncFn() + Send + Sync,
    preview_zoom: impl Fn() + Send + Sync,
    rebuild: impl Fn(bool, bool) + Send + Sync,
) {
    let mut sort_fired = false;
    let mut zoom_fired = false;
    loop {
        select! {
            r = sort_rx.recv() => {
                if r.is_err() { return; }
                sort_fired = true;
            }
            r = zoom_rx.recv() => {
                if r.is_err() { return; }
                zoom_fired = true;
            }
        }
        if !sort_fired && zoom_fired && sort_rx.is_empty() {
            preview_zoom();
        }
        coalesce_quiet_period(
            &sort_rx,
            &zoom_rx,
            &mut sort_fired,
            &mut zoom_fired,
            &debounce,
        )
        .await;
        rebuild(sort_fired, zoom_fired);
        sort_fired = false;
        zoom_fired = false;
    }
}

/// Extend the debounce window until a full quiet period passes with no new
/// sort/zoom changes.
///
/// Folds any changes arriving mid-window into the fired flags so the rebuild
/// handles them in a single pass. Buffered messages are drained so a burst
/// does not re-trigger the loop on the next poll.
async fn coalesce_quiet_period(
    sort_rx: &Receiver<()>,
    zoom_rx: &Receiver<()>,
    sort_fired: &mut bool,
    zoom_fired: &mut bool,
    debounce: &(impl AsyncFn() + Send + Sync),
) {
    loop {
        debounce().await;
        let mut sort_new = false;
        while sort_rx.try_recv().is_ok() {
            sort_new = true;
        }
        let mut zoom_new = false;
        while zoom_rx.try_recv().is_ok() {
            zoom_new = true;
        }
        *sort_fired |= sort_new;
        *zoom_fired |= zoom_new;
        if !sort_new && !zoom_new {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use {
        anyhow::{Error, Result, ensure},
        async_channel::{Receiver, Sender, unbounded},
        parking_lot::Mutex,
        tokio::{spawn, task::JoinHandle, test as tokio_test, time::sleep},
    };

    use crate::{
        storage::{
            active_tab::ActiveTab::{Albums, Artists},
            view_mode::ViewMode::{Column, Grid},
        },
        ui::gallery::rebuild_debounce::{
            RebuildAction::{DeferDirty, Rebuild, Resize},
            SORT_ZOOM_DEBOUNCE, decide_rebuild, listen_sort_zoom_loop,
        },
    };

    type ListenLoopHarness = (
        Arc<Mutex<Vec<(bool, bool)>>>,
        Arc<Mutex<Vec<()>>>,
        Sender<()>,
        Sender<()>,
        JoinHandle<()>,
    );

    async fn run_listen_loop(
        sort_rx: Receiver<()>,
        zoom_rx: Receiver<()>,
        calls: Arc<Mutex<Vec<(bool, bool)>>>,
        previews: Arc<Mutex<Vec<()>>>,
    ) {
        listen_sort_zoom_loop(
            sort_rx,
            zoom_rx,
            async || {
                sleep(SORT_ZOOM_DEBOUNCE).await;
            },
            move || {
                previews.lock().push(());
            },
            move |sort_fired, zoom_fired| {
                calls.lock().push((sort_fired, zoom_fired));
            },
        )
        .await;
    }

    fn start_listen_loop() -> ListenLoopHarness {
        let (sort_tx, sort_rx) = unbounded();
        let (zoom_tx, zoom_rx) = unbounded();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let previews = Arc::new(Mutex::new(Vec::new()));
        let task = spawn(run_listen_loop(
            sort_rx,
            zoom_rx,
            Arc::clone(&calls),
            Arc::clone(&previews),
        ));
        (calls, previews, sort_tx, zoom_tx, task)
    }

    async fn send_zoom_burst(zoom_tx: &Sender<()>) -> Result<()> {
        zoom_tx.try_send(()).map_err(Error::msg)?;
        zoom_tx.try_send(()).map_err(Error::msg)?;
        sleep(SORT_ZOOM_DEBOUNCE).await;
        sleep(SORT_ZOOM_DEBOUNCE).await;
        sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    #[test]
    fn decide_rebuild_defers_when_tab_is_hidden() {
        let cases = [
            (Artists, Albums, true, false, false),
            (Artists, Albums, false, true, true),
            (Albums, Artists, true, false, false),
            (Albums, Artists, false, true, true),
        ];
        for (active, tab, sort_fired, zoom_fired, ready) in cases {
            assert_eq!(
                decide_rebuild(active, tab, Grid, sort_fired, zoom_fired, ready),
                DeferDirty,
                "hidden tab must defer regardless of change type"
            );
        }
    }

    #[test]
    fn decide_rebuild_resizes_only_zoom_only_changes_on_ready_grid() {
        assert_eq!(
            decide_rebuild(Albums, Albums, Grid, false, true, true),
            Resize,
            "ready grid with zoom-only change must resize"
        );
        assert_eq!(
            decide_rebuild(Albums, Albums, Grid, true, true, true),
            Rebuild,
            "a sort change forces a rebuild even with zoom"
        );
        assert_eq!(
            decide_rebuild(Albums, Albums, Grid, false, true, false),
            Rebuild,
            "unready grid must rebuild instead of resizing"
        );
        assert_eq!(
            decide_rebuild(Albums, Albums, Column, false, true, true),
            Rebuild,
            "column mode has no in-place zoom resize"
        );
        assert_eq!(
            decide_rebuild(Albums, Albums, Grid, false, false, true),
            Rebuild,
            "a plain rebuild signal must not resize"
        );
    }

    #[tokio_test(start_paused = true)]
    async fn listen_sort_zoom_loop_rebuilds_once_per_change() -> Result<()> {
        let (calls, previews, sort_tx, zoom_tx, task) = start_listen_loop();

        sort_tx.try_send(()).map_err(Error::msg)?;
        sleep(SORT_ZOOM_DEBOUNCE).await;
        sleep(Duration::from_millis(10)).await;
        ensure!(
            calls.lock().clone() == vec![(true, false)],
            "single sort change must rebuild exactly once"
        );
        ensure!(
            previews.lock().is_empty(),
            "a sort change must not run the zoom preview"
        );

        zoom_tx.try_send(()).map_err(Error::msg)?;
        sleep(SORT_ZOOM_DEBOUNCE).await;
        sleep(Duration::from_millis(10)).await;
        ensure!(
            calls.lock().clone() == vec![(true, false), (false, true)],
            "zoom change must trigger a second rebuild"
        );
        ensure!(
            previews.lock().len() == 1,
            "zoom change must preview before the debounced rebuild"
        );

        drop(sort_tx);
        drop(zoom_tx);
        task.await?;
        Ok(())
    }

    #[tokio_test(start_paused = true)]
    async fn listen_sort_zoom_loop_coalesces_burst_into_one_rebuild() -> Result<()> {
        let (calls, previews, sort_tx, zoom_tx, task) = start_listen_loop();

        sort_tx.try_send(()).map_err(Error::msg)?;
        send_zoom_burst(&zoom_tx).await?;

        ensure!(
            calls.lock().clone() == vec![(true, true)],
            "burst within the debounce window must coalesce to a single rebuild"
        );
        ensure!(
            previews.lock().is_empty(),
            "a combined sort+zoom burst must take the full rebuild, not the preview"
        );

        drop(sort_tx);
        drop(zoom_tx);
        task.await?;
        Ok(())
    }

    #[tokio_test(start_paused = true)]
    async fn listen_sort_zoom_loop_previews_only_first_zoom_in_burst() -> Result<()> {
        let (calls, previews, sort_tx, zoom_tx, task) = start_listen_loop();

        send_zoom_burst(&zoom_tx).await?;

        ensure!(
            previews.lock().len() == 1,
            "only the first zoom in a burst needs an immediate preview"
        );
        ensure!(
            calls.lock().clone() == vec![(false, true)],
            "the whole burst must coalesce into a single rebuild"
        );

        drop(sort_tx);
        drop(zoom_tx);
        task.await?;
        Ok(())
    }
}

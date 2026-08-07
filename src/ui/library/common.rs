//! Shared batched-population helpers for library grid views.

use std::{mem::take, sync::Arc, time::Duration};

use {
    async_channel::Receiver,
    libadwaita::{
        gdk::Key,
        glib::{
            ControlFlow::{self, Break, Continue},
            Propagation::{Proceed, Stop},
            idle_add_local, spawn_future_local, timeout_future,
        },
        gtk::{
            Align::{Center, Start},
            EventControllerKey, FlowBox, Image, Overlay, Picture,
            PropagationPhase::Capture,
            ScrolledWindow,
            SelectionMode::None as SelectionNone,
            Stack,
            accessible::Property::Label,
        },
        prelude::{AccessibleExtManual, Cast, EventControllerExt, WidgetExt},
    },
    parking_lot::Mutex,
    tokio::select,
};

use crate::{
    app::{AppState, NavigationEvent, SortMemo},
    storage::settings::{
        ActiveTab,
        ViewMode::{self, Column, Grid},
    },
    zoom::grid_cover_size,
};

/// Debounce window for coalescing rapid sort/zoom changes before a grid rebuild.
pub const SORT_ZOOM_DEBOUNCE: Duration = Duration::from_millis(200);

/// Number of cards to build per idle callback batch.
pub const GRID_BATCH_SIZE: usize = 10;

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
    debounce: impl AsyncFn(),
    preview_zoom: impl Fn(),
    rebuild: impl AsyncFn(bool, bool),
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
        rebuild(sort_fired, zoom_fired).await;
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
    debounce: &impl AsyncFn(),
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

/// Spawn a future that listens for sort-config and zoom changes and rebuilds the grid.
///
/// Subscribes to the given per-entity sort receiver and zoom receiver,
/// coalescing rapid changes with the [`SORT_ZOOM_DEBOUNCE`] window. The zoom
/// receiver must be this grid's own channel (see [`AppState::albums_zoom_rx`]
/// / [`AppState::artists_zoom_rx`]) — `async_channel` receivers compete for
/// messages, so sharing one channel across grids would drop every other zoom
/// notification for whichever grid lost the race. The debounce is driven by a
/// glib-native timer future so the whole loop stays on the `GLib` main context
/// without depending on tokio wakers. `preview_zoom` runs immediately on a
/// zoom-only change so cards resize before the debounce; the rebuild handler
/// receives `(sort_fired, zoom_fired)` flags so it can resize in place for
/// zoom-only changes.
pub fn spawn_listen_sort_zoom(
    state: &Arc<AppState>,
    sort_rx: Receiver<()>,
    zoom_rx: Receiver<()>,
    preview_zoom: impl Fn() + 'static,
    rebuild: impl AsyncFn(bool, bool) + 'static,
) {
    let storage = Arc::clone(&state.storage);
    spawn_future_local(async move {
        listen_sort_zoom_loop(
            sort_rx,
            zoom_rx,
            async || {
                timeout_future(SORT_ZOOM_DEBOUNCE).await;
            },
            preview_zoom,
            async move |sort_fired, zoom_fired| {
                rebuild(sort_fired, zoom_fired).await;
                storage.save_settings();
            },
        )
        .await;
    });
}

/// Locate the live `FlowBox` inside a mode stack's `"grid"` child.
///
/// The grid child is a `ScrolledWindow` wrapping a vertical container that
/// holds the `FlowBox`. `GtkScrolledWindow` auto-wraps a non-scrollable
/// child (the grid's `GtkBox`) in a `GtkViewport`, so this walks the
/// first-child chain from the scrolled window's child until it reaches
/// the `FlowBox`. Returns `None` when the grid view is absent.
#[must_use]
pub fn grid_flow_box(mode_stack: &Stack) -> Option<FlowBox> {
    let scrolled = mode_stack.child_by_name("grid")?;
    let Ok(scrolled) = scrolled.downcast::<ScrolledWindow>() else {
        return None;
    };
    let mut current = scrolled.child()?;
    loop {
        if let Some(flow) = current.downcast_ref::<FlowBox>() {
            return Some(flow.clone());
        }
        current = current.first_child()?;
    }
}

/// Get the cover/avatar `Overlay` for the `index`-th card in a `FlowBox`.
///
/// `GtkFlowBox` wraps each appended widget in an internal `GtkFlowBoxChild`,
/// so the wrapper's first child is the card; the card's first child is the
/// cover/avatar `Overlay`. Returns `None` for a missing or unexpected child.
#[must_use]
pub fn flowbox_card_overlay(flow: &FlowBox, index: i32) -> Option<Overlay> {
    let wrapper = flow.child_at_index(index)?;
    let card = wrapper.first_child()?;
    let Ok(overlay) = card.first_child()?.downcast::<Overlay>() else {
        return None;
    };
    Some(overlay)
}

/// Locate the grid's `FlowBox`, compute the current cover size, and apply the
/// matching spacing. Returns `None` when the grid view is absent.
#[must_use]
pub fn grid_resize_context(state: &Arc<AppState>, mode_stack: &Stack) -> Option<(FlowBox, i32)> {
    let flow = grid_flow_box(mode_stack)?;
    let cover_size = grid_cover_size(state.storage.get_grid_zoom_level());
    apply_grid_spacing(&flow, cover_size);
    Some((flow, cover_size))
}

/// Schedule an in-place zoom resize of a live grid's cards across idle callbacks.
///
/// Applies the matching `FlowBox` spacing synchronously (cheap), then resizes
/// up to [`GRID_BATCH_SIZE`] card overlays per idle callback so the frame
/// clock keeps scheduling windows during a zoom on large libraries — the
/// synchronous full-grid pass would otherwise starve the main loop. The
/// traversal bails out when `is_stale` reports the live build generation has
/// moved past the one captured at schedule time (a newer build superseded
/// this resize).
///
/// `resolve_card` runs for each card after its geometry is resized — the
/// album grid uses it to resolve cover art to the new size, the artist grid
/// passes a no-op. The resized card overlays are collected in order and
/// passed (alongside the cover size used) to `on_complete` when the
/// traversal finishes, so the caller can dispatch cover decoding without
/// re-walking the grid.
///
/// # Arguments
///
/// * `state` - Application state (zoom level source)
/// * `mode_stack` - The grid's mode stack
/// * `is_stale` - Predicate that reports whether a newer build superseded this resize
/// * `resolve_card` - Per-card action run after the geometry resize
/// * `on_complete` - Runs once with all resized overlays when the traversal finishes
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// grid view is absent (the caller falls back to a full rebuild).
pub fn resize_grid_batched<F, G>(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    is_stale: impl Fn() -> bool + 'static,
    resolve_card: F,
    on_complete: G,
) -> bool
where
    F: FnMut(usize, &Overlay, i32) + 'static,
    G: FnOnce(Vec<Overlay>, i32) + 'static,
{
    let Some((flow, cover_size)) = grid_resize_context(state, mode_stack) else {
        return false;
    };
    let mut resolve_card = resolve_card;
    let mut on_complete = Some(on_complete);
    let mut next = 0usize;
    let mut collected: Vec<Overlay> = Vec::new();
    idle_add_local(move || {
        if is_stale() {
            return Break;
        }
        if resize_next_batch(
            &flow,
            cover_size,
            &mut next,
            &mut collected,
            &mut resolve_card,
        ) {
            finish_resize(&mut on_complete, &mut collected, cover_size);
            Break
        } else {
            Continue
        }
    });
    true
}

/// Process up to [`GRID_BATCH_SIZE`] cards for a scheduled grid resize.
///
/// Resizes each card's cover/avatar in place and runs `resolve_card`, then
/// records the overlay for the completion pass. Returns `true` once the
/// grid's cards are exhausted (or the index overflows `i32`, which cannot
/// happen for a real grid).
fn resize_next_batch<F>(
    flow: &FlowBox,
    cover_size: i32,
    next: &mut usize,
    collected: &mut Vec<Overlay>,
    resolve_card: &mut F,
) -> bool
where
    F: FnMut(usize, &Overlay, i32),
{
    let start = *next;
    let end = start + GRID_BATCH_SIZE;
    *next = end;
    for idx in start..end {
        let index = i32::try_from(idx).unwrap_or(i32::MAX);
        let Some(overlay) = flowbox_card_overlay(flow, index) else {
            return true;
        };
        resize_overlay_cover(&overlay, cover_size);
        resolve_card(idx, &overlay, cover_size);
        collected.push(overlay);
    }
    false
}

/// Run a grid resize's completion callback with the collected overlays.
fn finish_resize<G>(on_complete: &mut Option<G>, collected: &mut Vec<Overlay>, cover_size: i32)
where
    G: FnOnce(Vec<Overlay>, i32),
{
    if let Some(on_complete) = on_complete.take() {
        on_complete(take(collected), cover_size);
    }
}

/// Pop up to [`GRID_BATCH_SIZE`] indices and invoke `build` for each.
///
/// Bails with `Break` when the build generation is stale (a newer build has
/// superseded this one); returns `Continue` when more items remain for the
/// next batch.
pub fn fill_grid_batch<F>(
    is_current: bool,
    state: &Arc<AppState>,
    remaining: &mut Vec<usize>,
    mut build: F,
) -> ControlFlow
where
    F: FnMut(usize, i32),
{
    if !is_current {
        return Break;
    }
    let size = grid_cover_size(state.storage.get_grid_zoom_level());
    for _ in 0..GRID_BATCH_SIZE {
        let Some(idx) = remaining.pop() else {
            break;
        };
        build(idx, size);
    }
    if remaining.is_empty() {
        Break
    } else {
        Continue
    }
}

/// Map a cover size to the grid's row/column spacing in pixels.
fn grid_spacing(cover_size: i32) -> u32 {
    (cover_size.max(0).cast_unsigned() * 12 + 90) / 180
}

/// Update a `FlowBox`'s row/column spacing to match `cover_size`.
pub fn apply_grid_spacing(flow: &FlowBox, cover_size: i32) {
    let spacing = grid_spacing(cover_size);
    flow.set_row_spacing(spacing);
    flow.set_column_spacing(spacing);
}

/// Resize a card's cover/avatar widget to `size` in place.
///
/// Handles both the placeholder `Image` and a decoded `Picture`, keeping
/// the existing widget tree intact so zoom never recreates the cards.
pub fn resize_overlay_cover(overlay: &Overlay, size: i32) {
    let Some(child) = overlay.child() else {
        return;
    };
    if let Some(img) = child.downcast_ref::<Image>() {
        img.set_pixel_size(size / 2);
        img.set_width_request(size);
        img.set_height_request(size);
        return;
    }
    if let Some(pic) = child.downcast_ref::<Picture>() {
        pic.set_width_request(size);
        pic.set_height_request(size);
    }
}

/// Build a configured `FlowBox` for grid-mode display.
///
/// Spacing is scaled proportionally based on the cover size
/// (180 px → 12 px spacing).
#[must_use]
pub fn build_grid(tooltip: &str, cover_size: i32) -> FlowBox {
    let spacing = grid_spacing(cover_size);
    let flow = FlowBox::builder()
        .min_children_per_line(2)
        .valign(Start)
        .halign(Center)
        .row_spacing(spacing)
        .column_spacing(spacing)
        .selection_mode(SelectionNone)
        .can_focus(true)
        .tooltip_text(tooltip)
        .build();
    flow.update_property(&[Label(tooltip)]);
    flow
}

/// Set up keyboard navigation (Enter/Space) on a `FlowBox` for accessibility.
/// `card_ids` must correspond to the order of children in the `FlowBox`.
pub fn setup_flowbox_keyboard_nav(
    flow: &FlowBox,
    state: &Arc<AppState>,
    card_ids: Vec<i64>,
    make_event: fn(i64) -> NavigationEvent,
) {
    let key_controller = EventControllerKey::new();
    key_controller.set_propagation_phase(Capture);
    let state_kb = Arc::clone(state);
    let flow_clone = flow.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key != Key::Return && key != Key::KP_Enter && key != Key::space {
            return Proceed;
        }
        activate_focused_card(&flow_clone, &state_kb, &card_ids, make_event);
        Stop
    });
    flow.add_controller(key_controller);
}

/// Activate the currently focused child of a `FlowBox`, navigating to its detail page.
fn activate_focused_card(
    flow: &FlowBox,
    state: &Arc<AppState>,
    card_ids: &[i64],
    make_event: fn(i64) -> NavigationEvent,
) {
    let mut i = 0i32;
    while let Some(child) = flow.child_at_index(i) {
        if child.has_focus() {
            activate_focused_card_by_index(state, card_ids, i, make_event);
            break;
        }
        i += 1;
    }
}

/// Navigate to the detail page for the card at the given `FlowBox` index.
fn activate_focused_card_by_index(
    state: &Arc<AppState>,
    card_ids: &[i64],
    index: i32,
    make_event: fn(i64) -> NavigationEvent,
) {
    if let Some(&id) = card_ids.get(usize::try_from(index).unwrap_or(0)) {
        let s = Arc::clone(state);
        spawn_future_local(async move {
            s.send_navigation_event(make_event(id)).await;
        });
    }
}

/// Remove the stale child for the current view mode if `tab` is the active tab.
///
/// # Arguments
///
/// * `state` - App state used to check the active tab and current view mode
/// * `tab` - The library tab that must be active for the rebuild
/// * `mode_stack` - The mode stack to remove the stale child from
///
/// # Returns
///
/// The current `ViewMode` if `tab` is active, otherwise `None`
pub fn take_stale_mode_child(
    state: &Arc<AppState>,
    tab: ActiveTab,
    mode_stack: &Stack,
) -> Option<ViewMode> {
    if *state.active_tab_tx.borrow() != tab {
        return None;
    }
    let mode = *state.view_mode_tx.borrow();
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    if let Some(child) = mode_stack.child_by_name(child_name) {
        mode_stack.remove(&child);
    }
    Some(mode)
}

/// Remove both grid and column mode children, so the next mode switch
/// rebuilds them lazily from the in-memory cache.
///
/// Used on sort changes, where every mode child reflects the previous sort
/// order and must be invalidated.
pub fn clear_mode_children(mode_stack: &Stack) {
    for name in ["grid", "column"] {
        if let Some(child) = mode_stack.child_by_name(name) {
            mode_stack.remove(&child);
        }
    }
}

/// Try to build a library mode from its in-memory cache.
///
/// Returns `true` when the cache was used (including the empty‑state case),
/// meaning the caller should return. The empty check, build, and empty‑state
/// presentation are provided by the caller so the album and artist grids
/// share this single flow.
pub fn try_build_from_cache<T>(
    cache: &Mutex<Option<T>>,
    stack: &Stack,
    child_name: &str,
    is_empty: impl Fn(&T) -> bool,
    build: impl FnOnce(&T),
    show_empty: impl FnOnce(),
) -> bool {
    let guard = cache.lock();
    let Some(cached) = guard.as_ref() else {
        return false;
    };
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return true;
    }
    if is_empty(cached) {
        drop(guard);
        show_empty();
    } else {
        build(cached);
        drop(guard);
        stack.set_visible_child_name(child_name);
    }
    true
}

/// Look up or compute the memoized display-order indices for a library grid.
///
/// Returns the memoized indices when the grid `generation` and the current
/// sort `config` both match, so repeated tab/mode switches reuse the sort
/// instead of re-comparing every item. Otherwise computes them with `compute`
/// (passed the current `config`), stores the result in `memo`, and returns it.
#[must_use]
pub fn memoized_sort_indices<C>(
    generation: u64,
    config: C,
    memo: &Mutex<Option<SortMemo<C>>>,
    compute: impl FnOnce(&C) -> Vec<usize>,
) -> Arc<[usize]>
where
    C: PartialEq,
{
    let mut memo = memo.lock();
    if let Some(m) = memo.as_ref()
        && m.generation == generation
        && m.config == config
    {
        return Arc::clone(&m.indices);
    }
    let indices: Arc<[usize]> = Arc::from(compute(&config));
    *memo = Some(SortMemo {
        generation,
        config,
        indices: Arc::clone(&indices),
    });
    indices
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use {
        anyhow::{Error, Result, ensure},
        async_channel::{Receiver, Sender, unbounded},
        libadwaita::{
            gio::prelude::ListModelExt,
            glib::{
                ControlFlow::{Break, Continue},
                MainContext,
            },
            gtk::{self, test},
            prelude::WidgetExt,
        },
        parking_lot::Mutex,
        tokio::{spawn, task::JoinHandle, test as tokio_test, time::sleep},
    };

    use crate::{
        app::{AppState, NavigationEvent::AlbumDetail, SortMemo},
        storage::settings::{
            ActiveTab::{Albums, Artists},
            ViewMode::{Column, Grid},
        },
        ui::library::common::{
            GRID_BATCH_SIZE,
            RebuildAction::{DeferDirty, Rebuild, Resize},
            SORT_ZOOM_DEBOUNCE, activate_focused_card_by_index, build_grid, decide_rebuild,
            fill_grid_batch, grid_spacing, listen_sort_zoom_loop, memoized_sort_indices,
            setup_flowbox_keyboard_nav,
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
            async move |sort_fired, zoom_fired| {
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

    fn memo_harness() -> Mutex<Option<SortMemo<u8>>> {
        Mutex::new(None)
    }

    fn sort_compute() -> impl FnOnce(&u8) -> Vec<usize> {
        |config: &u8| vec![usize::from(*config), 9]
    }

    #[test]
    fn build_grid_sets_tooltip() -> Result<()> {
        let flow = build_grid("Album library grid", 180);
        ensure!(flow.tooltip_text().as_deref() == Some("Album library grid"));
        Ok(())
    }

    #[test]
    fn setup_flowbox_keyboard_nav_attaches_key_controller() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let flow = build_grid("grid", 180);
        setup_flowbox_keyboard_nav(&flow, &state, vec![1, 2], AlbumDetail);
        ensure!(flow.observe_controllers().n_items() > 0);
        Ok(())
    }

    #[test]
    fn activate_focused_card_by_index_sends_album_detail() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        activate_focused_card_by_index(&state, &[7, 8], 1, AlbumDetail);
        let mut received = state.navigation_rx.try_recv();
        let mut attempts = 0;
        while received.is_err() && attempts < 32 {
            MainContext::default().iteration(false);
            received = state.navigation_rx.try_recv();
            attempts += 1;
        }
        ensure!(matches!(received, Ok(AlbumDetail(8))));
        Ok(())
    }

    #[test]
    fn activate_focused_card_by_index_out_of_bounds_is_noop() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        activate_focused_card_by_index(&state, &[7], 5, AlbumDetail);
        MainContext::default().iteration(false);
        ensure!(state.navigation_rx.try_recv().is_err());
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

    #[test]
    fn fill_grid_batch_bails_when_build_is_stale() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let mut remaining = vec![0usize, 1, 2];
        let mut built = 0;
        let flow = fill_grid_batch(false, &state, &mut remaining, |_, _| built += 1);
        ensure!(flow == Break, "stale build must bail");
        ensure!(built == 0, "stale build must not build any cards");
        ensure!(
            remaining == vec![0, 1, 2],
            "stale build must not consume indices"
        );
        Ok(())
    }

    #[test]
    fn fill_grid_batch_builds_up_to_batch_size_then_continues() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let mut remaining: Vec<usize> = (0..GRID_BATCH_SIZE + 3).collect();
        let mut built = 0;
        let flow = fill_grid_batch(true, &state, &mut remaining, |_, _| built += 1);
        ensure!(flow == Continue, "more indices remain, must continue");
        ensure!(built == GRID_BATCH_SIZE, "must build exactly one batch");
        ensure!(remaining.len() == 3, "batch must consume one batch worth");
        Ok(())
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

    #[test]
    fn grid_spacing_scales_with_cover_size() {
        let cases = [(0, 0), (120, 8), (150, 10), (180, 12), (210, 14), (240, 16)];
        for (cover_size, expected) in cases {
            assert_eq!(
                grid_spacing(cover_size),
                expected,
                "cover size {cover_size} px must map to spacing {expected} px"
            );
        }
    }

    #[test]
    fn grid_spacing_clamps_negative_cover_sizes() {
        assert_eq!(grid_spacing(-100), 0);
        assert_eq!(grid_spacing(-1), 0);
    }

    #[test]
    fn memoized_sort_indices_reuses_cache_without_recompute() {
        let memo = memo_harness();

        let first = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        let second = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        assert!(
            Arc::ptr_eq(&first, &second),
            "same generation and config must return the memoized indices"
        );
        assert_eq!(&*first, &[5, 9]);
    }

    #[test]
    fn memoized_sort_indices_recomputes_on_generation_change() {
        let memo = memo_harness();

        let first = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        let second = memoized_sort_indices(2, 5u8, &memo, sort_compute());
        assert!(
            !Arc::ptr_eq(&first, &second),
            "a generation bump must invalidate the memoized indices"
        );
    }

    #[test]
    fn memoized_sort_indices_recomputes_on_config_change() {
        let memo = memo_harness();

        let first = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        let second = memoized_sort_indices(1, 7u8, &memo, sort_compute());
        assert!(
            !Arc::ptr_eq(&first, &second),
            "a different config must invalidate the memoized indices"
        );
        assert_eq!(&*second, &[7, 9]);
    }
}

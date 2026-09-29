//! Batched `FlowBox` population and in-place resize traversal for library grids.
//!
//! Locates the live grid, applies spacing, and walks cards in small idle
//! batches so large libraries never starve the frame clock during builds,
//! zoom resizes, or narrow-window fit resizes.

use std::{mem::take, sync::Arc};

use libadwaita::{
    glib::{
        ControlFlow::{self, Break, Continue},
        idle_add_local,
    },
    gtk::{FlowBox, Overlay, ScrolledWindow, Stack},
    prelude::{Cast, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    ui::gallery::grid_flow::{apply_grid_spacing, resize_overlay_cover},
};

/// Number of cards to build per idle callback batch.
pub const GRID_BATCH_SIZE: usize = 10;

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

/// Locate the grid's `FlowBox` and apply the matching spacing.
///
/// # Arguments
///
/// * `mode_stack` - The grid's mode stack holding the `"grid"` child.
/// * `cover_size` - Cover size to match the spacing to (already resolved for narrow windows by the
///   caller).
///
/// # Returns
///
/// The `FlowBox` and the cover size, or `None` when the grid view is absent.
#[must_use]
pub fn grid_resize_context(mode_stack: &Stack, cover_size: i32) -> Option<(FlowBox, i32)> {
    let flow = grid_flow_box(mode_stack)?;
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
/// * `state` - Application state (idle-source owner)
/// * `mode_stack` - The grid's mode stack
/// * `cover_size` - Cover size to resize to (already resolved for narrow windows)
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
    cover_size: i32,
    is_stale: impl Fn() -> bool + 'static,
    resolve_card: F,
    on_complete: G,
) -> bool
where
    F: FnMut(usize, &Overlay, i32) + 'static,
    G: FnOnce(Vec<Overlay>, i32) + 'static,
{
    let Some((flow, cover_size)) = grid_resize_context(mode_stack, cover_size) else {
        return false;
    };
    let mut resolve_card = resolve_card;
    let mut on_complete = Some(on_complete);
    let mut next = 0usize;
    let mut collected: Vec<Overlay> = Vec::new();
    state.handles.lock().retain_source(idle_add_local(move || {
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
    }));
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
    let end = start.saturating_add(GRID_BATCH_SIZE);
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
///
/// # Arguments
///
/// * `is_current` - Whether the build generation is still current.
/// * `cover_size` - Cover size to build cards at (already resolved for narrow windows by the
///   caller).
/// * `remaining` - Indices still to build, popped from the back.
/// * `build` - Per-index card builder receiving `(index, cover_size)`.
pub fn fill_grid_batch<F>(
    is_current: bool,
    cover_size: i32,
    remaining: &mut Vec<usize>,
    mut build: F,
) -> ControlFlow
where
    F: FnMut(usize, i32),
{
    if !is_current {
        return Break;
    }
    for _ in 0..GRID_BATCH_SIZE {
        let Some(idx) = remaining.pop() else {
            break;
        };
        build(idx, cover_size);
    }
    if remaining.is_empty() {
        Break
    } else {
        Continue
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::glib::ControlFlow::{Break, Continue},
    };

    use crate::ui::gallery::grid_batch::{GRID_BATCH_SIZE, fill_grid_batch};

    #[test]
    fn fill_grid_batch_bails_when_build_is_stale() -> Result<()> {
        let mut remaining = vec![0usize, 1, 2];
        let mut built: usize = 0;
        let flow = fill_grid_batch(false, 180, &mut remaining, |_, _| {
            built = built.saturating_add(1);
        });
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
        let mut remaining: Vec<usize> = (0..GRID_BATCH_SIZE + 3).collect();
        let mut built: usize = 0;
        let mut seen_size = 0;
        let flow = fill_grid_batch(true, 150, &mut remaining, |_, size| {
            built = built.saturating_add(1);
            seen_size = size;
        });
        ensure!(flow == Continue, "more indices remain, must continue");
        ensure!(built == GRID_BATCH_SIZE, "must build exactly one batch");
        ensure!(remaining.len() == 3, "batch must consume one batch worth");
        ensure!(
            seen_size == 150,
            "batch must build cards at the requested cover size"
        );
        Ok(())
    }
}

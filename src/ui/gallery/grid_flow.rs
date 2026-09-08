//! `FlowBox` build, layout, and batched population helpers for library grids.

use std::{mem::take, sync::Arc};

use libadwaita::{
    glib::{
        ControlFlow::{self, Break, Continue},
        idle_add_local,
    },
    gtk::{
        Align::{Center, Start},
        FlowBox, Image, Overlay, Picture, ScrolledWindow,
        SelectionMode::None as SelectionNone,
        Stack,
        accessible::Property::Label,
    },
    prelude::{AccessibleExtManual, Cast, WidgetExt},
};

use crate::{app::runtime::AppState, ui::zoom::grid_cover_size};

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

/// Locate the grid's `FlowBox`, compute the current cover size, and apply the
/// matching spacing. Returns `None` when the grid view is absent.
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
    (cover_size
        .max(0)
        .cast_unsigned()
        .saturating_mul(12)
        .saturating_add(90))
        / 180
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
    if let Some(card) = overlay.parent() {
        card.set_width_request(size);
    }
    overlay.set_width_request(size);
    overlay.set_height_request(size);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            glib::{ControlFlow, prelude::Cast},
            gtk::{self, Box, Image, Orientation::Vertical, Overlay, test},
            prelude::{BoxExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::gallery::{
            card::build_placeholder,
            grid_flow::{
                GRID_BATCH_SIZE, build_grid, fill_grid_batch, grid_spacing, resize_overlay_cover,
            },
        },
    };

    #[test]
    fn build_grid_sets_tooltip() -> Result<()> {
        let flow = build_grid("Album library grid", 180);
        ensure!(flow.tooltip_text().as_deref() == Some("Album library grid"));
        Ok(())
    }

    #[test]
    fn fill_grid_batch_bails_when_build_is_stale() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let mut remaining = vec![0usize, 1, 2];
        let mut built: usize = 0;
        let flow = fill_grid_batch(false, &state, &mut remaining, |_, _| {
            built = built.saturating_add(1);
        });
        ensure!(flow == ControlFlow::Break, "stale build must bail");
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
        let mut built: usize = 0;
        let flow = fill_grid_batch(true, &state, &mut remaining, |_, _| {
            built = built.saturating_add(1);
        });
        ensure!(
            flow == ControlFlow::Continue,
            "more indices remain, must continue"
        );
        ensure!(built == GRID_BATCH_SIZE, "must build exactly one batch");
        ensure!(remaining.len() == 3, "batch must consume one batch worth");
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
    fn resize_overlay_cover_resizes_card_and_cover() -> Result<()> {
        let card = Box::new(Vertical, 0);
        let cover = build_placeholder(120);
        let overlay = Overlay::new();
        overlay.set_child(Some(&cover));
        card.append(&overlay);
        resize_overlay_cover(&overlay, 240);

        ensure!(
            card.width_request() == 240,
            "card width must follow the cover size"
        );
        ensure!(
            overlay.width_request() == 240,
            "overlay width must follow the cover size"
        );
        ensure!(
            overlay.height_request() == 240,
            "overlay height must follow the cover size"
        );
        let child = overlay.child();
        let Some(img) = child.as_ref().and_then(|w| w.downcast_ref::<Image>()) else {
            bail!("placeholder cover must be an Image");
        };
        ensure!(
            img.pixel_size() == 120,
            "placeholder icon must scale with the cover size"
        );
        ensure!(
            img.width_request() == 240,
            "placeholder width must follow the cover size"
        );
        ensure!(
            img.height_request() == 240,
            "placeholder height must follow the cover size"
        );
        Ok(())
    }
}

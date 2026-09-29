//! Narrow-window fit decisions for library grids.
//!
//! Resolves the cover size to render from the stored zoom level plus the
//! narrow flag, reports what a grid currently renders, and upgrades deferred
//! rebuilds to in-place resizes when the change is zoom-only on a built grid.

use libadwaita::{
    gtk::{FlowBox, Stack},
    prelude::WidgetExt,
};

use crate::{
    storage::view_mode::ViewMode::{self, Grid},
    ui::gallery::{
        grid_batch::{flowbox_card_overlay, grid_flow_box},
        rebuild_debounce::RebuildAction::{self, DeferDirty, Resize},
    },
};

/// Report the cover size the grid's cards currently render at.
///
/// Reads the first card's cover `Overlay` minimum width. Returns `None` for an
/// empty grid or an unexpected widget tree, in which case callers resize
/// unconditionally.
///
/// # Arguments
///
/// * `flow` - The grid's `FlowBox`.
///
/// # Returns
///
/// The rendered cover size in pixels, if a card is present.
#[must_use]
pub fn grid_rendered_size(flow: &FlowBox) -> Option<i32> {
    flowbox_card_overlay(flow, 0).map(|overlay| overlay.width_request())
}

/// Report whether the grid already renders cards at `cover_size`.
///
/// Used to skip no-op resizes: zoom clicks that snap to the same narrowed
/// size (and duplicate narrow notifications) must not walk the grid.
///
/// # Arguments
///
/// * `mode_stack` - The grid's mode stack holding the `"grid"` child.
/// * `cover_size` - Cover size to compare against.
///
/// # Returns
///
/// `true` when a card is present and renders at `cover_size`.
#[must_use]
pub fn grid_is_current(mode_stack: &Stack, cover_size: i32) -> bool {
    grid_flow_box(mode_stack).is_some_and(|flow| grid_rendered_size(&flow) == Some(cover_size))
}

/// Reconsider a deferred rebuild as an in-place resize when the change is
/// zoom-only on a built grid.
///
/// Hidden tabs normally defer work until shown, but a zoom-only change needs
/// no new data: resizing the already-built grid in place keeps hidden pages —
/// which homogeneous stacks still measure — from forcing the window past its
/// minimum width. Sort changes, unbuilt grids, and column mode keep deferring.
///
/// # Arguments
///
/// * `action` - The rebuild decision for this change.
/// * `sort_fired` - Whether a sort change is coalesced into this event.
/// * `zoom_fired` - Whether a zoom change is coalesced into this event.
/// * `view_mode` - The current library view mode.
/// * `ready` - Whether the grid finished populating.
/// * `mode_stack` - The grid's mode stack holding the `"grid"` child.
///
/// # Returns
///
/// [`Resize`](RebuildAction::Resize) when an in-place resize applies,
/// otherwise `action` unchanged.
#[must_use]
pub fn upgrade_hidden_resize(
    action: RebuildAction,
    sort_fired: bool,
    zoom_fired: bool,
    view_mode: ViewMode,
    ready: bool,
    mode_stack: &Stack,
) -> RebuildAction {
    if action == DeferDirty
        && !sort_fired
        && zoom_fired
        && view_mode == Grid
        && ready
        && mode_stack.child_by_name("grid").is_some()
    {
        Resize
    } else {
        action
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{
                self, Box, FlowBox,
                Orientation::{Horizontal, Vertical},
                Overlay, ScrolledWindow, Stack, test,
            },
            prelude::{BoxExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::{
            catalog::Album,
            formats::FormatInfo,
            view_mode::ViewMode::{Column, Grid},
        },
        ui::{
            gallery::{
                card::build_album_card,
                grid_fit::{grid_is_current, grid_rendered_size, upgrade_hidden_resize},
                grid_flow::build_grid,
                rebuild_debounce::RebuildAction::{DeferDirty, Rebuild, Resize},
            },
            window_geometry::MIN_WINDOW_WIDTH,
            zoom::{NARROW_GRID_ZOOM_MAX, grid_cover_size},
        },
    };

    fn append_overlay_card(flow: &FlowBox, size: i32) {
        let card = Box::new(Vertical, 0);
        let overlay = Overlay::builder()
            .width_request(size)
            .height_request(size)
            .build();
        card.append(&overlay);
        FlowBox::append(flow, &card);
    }

    #[test]
    fn narrowed_two_column_row_fits_minimum_window() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let size = grid_cover_size(NARROW_GRID_ZOOM_MAX);
        let flow = build_grid("grid", size);
        for (id, title) in [(1, "First Album"), (2, "Second Album")] {
            let album = Album {
                id,
                title: title.into(),
                artist_id: 1,
                year: Some(2024),
                genre: None,
                artwork_path: None,
                track_count: 10,
                total_duration: 3000.0,
                format_summary: "FLAC".into(),
                lossless: true,
                format: "FLAC".into(),
                bit_depth: Some(16),
                sample_rate: Some(44100),
            };
            let (card, _) =
                build_album_card(&state, &album, "Test Artist", &FormatInfo::default(), size);
            FlowBox::append(&flow, &card);
        }
        let (minimum, _, _, _) = flow.measure(Horizontal, -1);
        ensure!(
            minimum <= MIN_WINDOW_WIDTH,
            "narrowed two-column row {minimum} px must fit the {MIN_WINDOW_WIDTH} px minimum \
             window"
        );
        Ok(())
    }

    #[test]
    fn grid_rendered_size_reports_first_card() -> Result<()> {
        let flow = build_grid("grid", 150);
        ensure!(
            grid_rendered_size(&flow).is_none(),
            "an empty grid must report no rendered size"
        );
        append_overlay_card(&flow, 150);
        ensure!(
            grid_rendered_size(&flow) == Some(150),
            "a populated grid must report its card size"
        );
        Ok(())
    }

    #[test]
    fn grid_is_current_matches_rendered_size() -> Result<()> {
        let stack = Stack::new();
        ensure!(
            !grid_is_current(&stack, 150),
            "a missing grid must not report current"
        );
        let flow = build_grid("grid", 150);
        append_overlay_card(&flow, 150);
        let scrolled = ScrolledWindow::new();
        scrolled.set_child(Some(&flow));
        drop(stack.add_named(&scrolled, Some("grid")));
        ensure!(
            grid_is_current(&stack, 150),
            "a matching size must report current"
        );
        ensure!(
            !grid_is_current(&stack, 180),
            "a different size must not report current"
        );
        Ok(())
    }

    #[test]
    fn upgrade_hidden_resize_enables_zoom_only_resize() {
        let stack = Stack::new();
        assert_eq!(
            upgrade_hidden_resize(DeferDirty, false, true, Grid, true, &stack),
            DeferDirty,
            "a hidden tab without a built grid must keep deferring"
        );
        let flow = build_grid("grid", 240);
        let scrolled = ScrolledWindow::new();
        scrolled.set_child(Some(&flow));
        drop(stack.add_named(&scrolled, Some("grid")));
        assert_eq!(
            upgrade_hidden_resize(DeferDirty, false, true, Grid, true, &stack),
            Resize,
            "a zoom-only change on a built hidden grid must resize in place"
        );
    }

    #[test]
    fn upgrade_hidden_resize_keeps_other_decisions() {
        let stack = Stack::new();
        let flow = build_grid("grid", 240);
        let scrolled = ScrolledWindow::new();
        scrolled.set_child(Some(&flow));
        drop(stack.add_named(&scrolled, Some("grid")));
        for (sort_fired, zoom_fired, mode, ready) in [
            (true, true, Grid, true),
            (false, false, Grid, true),
            (false, true, Column, true),
            (false, true, Grid, false),
        ] {
            assert_eq!(
                upgrade_hidden_resize(DeferDirty, sort_fired, zoom_fired, mode, ready, &stack),
                DeferDirty,
                "sorts, non-zoom events, column mode, and unbuilt grids must keep deferring"
            );
        }
        assert_eq!(
            upgrade_hidden_resize(Resize, false, true, Grid, true, &stack),
            Resize,
            "an active-tab resize must pass through unchanged"
        );
        assert_eq!(
            upgrade_hidden_resize(Rebuild, true, false, Grid, true, &stack),
            Rebuild,
            "a rebuild must pass through unchanged"
        );
    }
}

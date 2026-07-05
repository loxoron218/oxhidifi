//! Shared batched-population helpers for library grid views.

use std::mem::take;

use libadwaita::{
    glib::{
        ControlFlow::{Break, Continue},
        idle_add_local,
    },
    gtk::{
        Align::{Center, Start},
        Box, FlowBox,
        SelectionMode::None,
        Widget,
    },
    prelude::BoxExt,
};

/// Build a configured `FlowBox` for grid-mode display.
#[must_use]
pub fn build_grid(tooltip: &str) -> FlowBox {
    FlowBox::builder()
        .min_children_per_line(2)
        .valign(Start)
        .halign(Center)
        .row_spacing(12)
        .column_spacing(12)
        .selection_mode(None)
        .can_focus(true)
        .tooltip_text(tooltip)
        .build()
}

/// Populate a `FlowBox` in grid mode with batched insertion for large libraries.
///
/// Adds cards to the container in batches via idle callbacks, allowing the
/// UI thread to process events between batches. This keeps the UI responsive
/// even with 10k+ items.
pub fn populate_grid_batched(
    container: &Box,
    cards: &mut Vec<Widget>,
    batch_size: usize,
    tooltip: &str,
) {
    let flow = build_grid(tooltip);
    container.append(&flow);

    let mut remaining = take(cards);
    idle_add_local(move || {
        let count = batch_size.min(remaining.len());
        for card in remaining.drain(..count) {
            flow.append(&card);
        }
        if remaining.is_empty() {
            Break
        } else {
            Continue
        }
    });
}

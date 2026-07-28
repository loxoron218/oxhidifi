//! Shared batched-population helpers for library grid views.

use std::{mem::take, sync::Arc};

use libadwaita::{
    gdk::Key,
    glib::{
        ControlFlow::{Break, Continue},
        Propagation::{Proceed, Stop},
        idle_add_local, spawn_future_local,
    },
    gtk::{
        Align::{Center, Start},
        Box, EventControllerKey, FlowBox,
        PropagationPhase::Capture,
        SelectionMode::None,
        Widget,
        accessible::Property::Label,
    },
    prelude::{AccessibleExtManual, BoxExt, EventControllerExt, WidgetExt},
};

use crate::app::{AppState, NavigationEvent};

/// Build a configured `FlowBox` for grid-mode display.
#[must_use]
pub fn build_grid(tooltip: &str) -> FlowBox {
    let flow = FlowBox::builder()
        .min_children_per_line(2)
        .valign(Start)
        .halign(Center)
        .row_spacing(12)
        .column_spacing(12)
        .selection_mode(None)
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

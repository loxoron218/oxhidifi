//! `FlowBox` keyboard navigation for grid views.

use std::sync::Arc;

use libadwaita::{
    gdk::Key,
    glib::{
        Propagation::{Proceed, Stop},
        spawn_future_local,
    },
    gtk::{EventControllerKey, FlowBox, PropagationPhase::Capture},
    prelude::{EventControllerExt, WidgetExt},
};

use crate::app::runtime::{AppState, NavigationEvent};

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
    state
        .handles
        .lock()
        .retain_signal(key_controller.connect_key_pressed(move |_, key, _, _| {
            if key != Key::Return && key != Key::KP_Enter && key != Key::space {
                return Proceed;
            }
            activate_focused_card(&flow_clone, &state_kb, &card_ids, make_event);
            Stop
        }));
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
        i = i.saturating_add(1);
    }
}

/// Navigate to the detail page for the card at the given `FlowBox` index.
pub fn activate_focused_card_by_index(
    state: &Arc<AppState>,
    card_ids: &[i64],
    index: i32,
    make_event: fn(i64) -> NavigationEvent,
) {
    if let Some(&id) = card_ids.get(usize::try_from(index).unwrap_or(0)) {
        let s = Arc::clone(state);
        state
            .handles
            .lock()
            .retain_task(spawn_future_local(async move {
                s.send_navigation_event(make_event(id)).await;
            }));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::MainContext,
            gtk::{self, test},
            prelude::{ListModelExt, WidgetExt},
        },
    };

    use crate::{
        app::{
            mocks::pump_in_test_runtime,
            runtime::{AppState, NavigationEvent::AlbumDetail},
        },
        ui::gallery::{
            grid_flow::build_grid,
            keyboard_nav::{activate_focused_card_by_index, setup_flowbox_keyboard_nav},
        },
    };

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
        pump_in_test_runtime(|| {
            for _ in 0..32 {
                if received.is_ok() {
                    break;
                }
                _ = MainContext::default().iteration(false);
                received = state.navigation_rx.try_recv();
            }
        })?;
        ensure!(matches!(received, Ok(AlbumDetail(8))));
        Ok(())
    }

    #[test]
    fn activate_focused_card_by_index_out_of_bounds_is_noop() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        activate_focused_card_by_index(&state, &[7], 5, AlbumDetail);
        pump_in_test_runtime(|| {
            _ = MainContext::default().iteration(false);
        })?;
        ensure!(state.navigation_rx.try_recv().is_err());
        Ok(())
    }
}

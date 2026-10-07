//! Sidebar toggle buttons and collapse-restore for the player panel.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

use libadwaita::{
    OverlaySplitView, glib::object::ObjectExt, gtk::ToggleButton, prelude::ToggleButtonExt,
};

use crate::app::runtime::AppState;

/// Synchronize the sidebar toggle buttons with the split view.
///
/// Both toggle buttons reflect and drive the sidebar visibility, and an
/// unconsumed collapse notification restores the user's last intent.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `split_view` - Split view whose sidebar is synchronized.
/// * `toggle_button` - Header toggle button for the player panel.
/// * `back_button` - Sidebar back button mirroring the toggle state.
/// * `sidebar_intent` - Shared last sidebar intent, updated by toggles and playback auto-show/hide
///   so collapse-restore agrees with it.
pub fn wire_sidebar_toggles(
    state: &Arc<AppState>,
    split_view: &OverlaySplitView,
    toggle_button: &ToggleButton,
    back_button: &ToggleButton,
    sidebar_intent: &Arc<AtomicBool>,
) {
    let user_wants_sidebar = Arc::clone(sidebar_intent);

    let sv = split_view.clone();
    let intended = Arc::clone(&user_wants_sidebar);
    state
        .handles
        .lock()
        .retain_signal(toggle_button.connect_toggled(move |btn| {
            intended.store(btn.is_active(), Relaxed);
            if sv.shows_sidebar() != btn.is_active() {
                sv.set_show_sidebar(btn.is_active());
            }
        }));

    let sv_back = split_view.clone();
    let intended_back = Arc::clone(&user_wants_sidebar);
    state
        .handles
        .lock()
        .retain_signal(back_button.connect_toggled(move |btn| {
            intended_back.store(btn.is_active(), Relaxed);
            if sv_back.shows_sidebar() != btn.is_active() {
                sv_back.set_show_sidebar(btn.is_active());
            }
        }));

    let intended_collapse = Arc::clone(&user_wants_sidebar);
    state
        .handles
        .lock()
        .retain_signal(split_view.connect_notify(Some("collapsed"), move |sv, _| {
            if sv.is_collapsed() {
                return;
            }
            let wants = intended_collapse.load(Relaxed);
            if sv.shows_sidebar() != wants {
                sv.set_show_sidebar(wants);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use libadwaita::{OverlaySplitView, gtk::ToggleButton};

    use crate::{app::runtime::AppState, ui::player::sidebar_toggles::wire_sidebar_toggles};

    #[test]
    fn wire_sidebar_toggles_signature_shape() {
        fn assert_shape<
            F: Fn(&Arc<AppState>, &OverlaySplitView, &ToggleButton, &ToggleButton, &Arc<AtomicBool>),
        >(
            _: F,
        ) {
        }
        assert_shape(wire_sidebar_toggles);
    }
}

//! Sidebar visibility shortcuts for the main window.
//!
//! Escape hides the player panel when shown; `Ctrl+B` toggles it. Keyval and
//! modifier matching live in the window's key controllers; persistence flows
//! through the existing `show_sidebar` notification.

use libadwaita::OverlaySplitView;

/// Hide the sidebar when Escape is pressed and the sidebar is shown.
///
/// Returns `true` when the key was handled (sidebar was visible and got
/// hidden), so the caller can stop propagation.
#[must_use]
pub fn handle_escape_key(split_view: &OverlaySplitView) -> bool {
    if split_view.shows_sidebar() {
        split_view.set_show_sidebar(false);
        true
    } else {
        false
    }
}

/// Toggle the sidebar visibility for the `Ctrl+B` shortcut.
///
/// Always flips the current state; the caller stops propagation,
/// synchronizes the shared sidebar intent, and persistence flows through the
/// existing `show_sidebar` notification. Key matching (keyval plus `Ctrl`
/// modifier) and the text-entry guard live in the window's key controller.
///
/// # Arguments
///
/// * `split_view` - Split view whose sidebar is toggled.
pub fn handle_sidebar_toggle(split_view: &OverlaySplitView) {
    split_view.set_show_sidebar(!split_view.shows_sidebar());
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            OverlaySplitView,
            gtk::{self, test as gtk_test},
        },
    };

    use crate::ui::key_bindings::sidebar_keys::{handle_escape_key, handle_sidebar_toggle};

    #[test]
    fn handler_signature_shapes() {
        fn assert_predicate<F: Fn(&OverlaySplitView) -> bool>(_: F) {}
        fn assert_toggle<F: Fn(&OverlaySplitView)>(_: F) {}
        assert_predicate(handle_escape_key);
        assert_toggle(handle_sidebar_toggle);
    }

    #[gtk_test]
    fn escape_key_hides_shown_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        ensure!(
            handle_escape_key(&split_view),
            "Escape on a shown sidebar must be handled"
        );
        ensure!(
            !split_view.shows_sidebar(),
            "Escape must hide the shown sidebar"
        );
        Ok(())
    }

    #[gtk_test]
    fn escape_key_ignores_hidden_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(false);
        ensure!(
            !handle_escape_key(&split_view),
            "Escape with a hidden sidebar must not be handled"
        );
        ensure!(!split_view.shows_sidebar());
        Ok(())
    }

    #[gtk_test]
    fn sidebar_toggle_hides_shown_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(true);
        handle_sidebar_toggle(&split_view);
        ensure!(
            !split_view.shows_sidebar(),
            "toggle must hide the shown sidebar"
        );
        Ok(())
    }

    #[gtk_test]
    fn sidebar_toggle_shows_hidden_sidebar() -> Result<()> {
        let split_view = OverlaySplitView::new();
        split_view.set_show_sidebar(false);
        handle_sidebar_toggle(&split_view);
        ensure!(
            split_view.shows_sidebar(),
            "toggle must show the hidden sidebar"
        );
        Ok(())
    }
}

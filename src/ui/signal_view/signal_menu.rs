//! Signal options for the main window header bar.
//!
//! The `Signal` tab owns no header bar or popover of its own. When it is
//! active the window header swaps its view-switch `SplitButton` for a plain
//! signal `MenuButton` carrying the copy action plus the shared preferences
//! entry. Output-settings and About shortcuts are intentionally omitted: the
//! generic preferences dialog already covers both landings.

use std::sync::Arc;

use {
    libadwaita::{
        gtk::{
            Box, Button, MenuButton,
            Orientation::{Horizontal, Vertical},
            Popover, Separator, Window,
            accessible::Property::Label,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    ui::{signal_view::signal_tab::SignalTab, toggle_popover::build_preferences_button},
};

/// Signal menu shown in the main header bar while the `Signal` tab is active.
#[derive(Debug, Clone)]
pub struct SignalHeaderMenu {
    /// Plain menu button replacing the view-switch control on `Signal`.
    button: MenuButton,
    /// Copies the retained path summary to the clipboard.
    copy: Button,
    /// Opens the shared preferences dialog.
    prefs: Button,
}

impl SignalHeaderMenu {
    /// Menu button for the main header bar.
    #[must_use]
    pub const fn menu_button(&self) -> &MenuButton {
        &self.button
    }

    /// Copy action button (test hook).
    #[must_use]
    pub const fn copy_button(&self) -> &Button {
        &self.copy
    }

    /// Preferences action button (test hook).
    #[must_use]
    pub const fn prefs_button(&self) -> &Button {
        &self.prefs
    }

    /// Number of actions parented in the popover (always two).
    #[must_use]
    pub fn action_count(&self) -> u32 {
        let parented = [&self.copy, &self.prefs]
            .iter()
            .filter(|button| button.parent().is_some())
            .count();
        u32::try_from(parented).unwrap_or(u32::MAX)
    }
}

/// Build one flat action button with an accessible name.
///
/// # Arguments
///
/// * `label` - Visible action text.
///
/// # Returns
///
/// * `Button` - Keyboard-focusable flat action button.
fn menu_action(label: &str) -> Button {
    let button = Button::builder()
        .label(label)
        .css_classes(["flat"])
        .can_focus(true)
        .hexpand(true)
        .build();
    button.update_property(&[Label(label)]);
    button
}

/// Build the header-bar signal menu for the `Signal` tab.
///
/// Creates a plain `MenuButton` whose popover carries the copy action plus
/// the shared preferences entry, and wires both actions. Copy writes the
/// tab's retained summary to the clipboard with a `Toast` confirmation;
/// preferences presents the existing preferences dialog.
///
/// # Arguments
///
/// * `state` - Application state for toasts and dialog settings.
/// * `parent` - Parent window used to present the preferences dialog.
/// * `tab` - Signal tab owning the retained path summary.
///
/// # Returns
///
/// * `SignalHeaderMenu` - Handles owning the `MenuButton` plus its actions.
#[must_use]
pub fn build_signal_header_menu(
    state: &Arc<AppState>,
    parent: &Window,
    tab: &SignalTab,
) -> SignalHeaderMenu {
    let copy = menu_action("Copy path summary");
    copy.set_tooltip_text(Some("Copy the signal path summary to the clipboard"));

    let prefs = build_preferences_button(state, parent);

    let list = Box::builder().orientation(Vertical).spacing(6).build();
    list.append(&copy);
    list.append(&Separator::new(Horizontal));
    list.append(&prefs);
    let popover = Popover::builder().child(&list).has_arrow(true).build();
    let button = MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Signal options")
        .can_focus(true)
        .popover(&popover)
        .build();
    button.update_property(&[Label("Signal options")]);

    let copy_tab = tab.clone();
    let copy_button = button.clone();
    let copy_toasts = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(copy.connect_clicked(move |_| {
            copy_button.popdown();
            let summary = copy_tab.pending_summary();
            copy_button.clipboard().set_text(&summary);
            if let Err(e) = copy_toasts
                .toast_tx
                .try_send(String::from("Signal path summary copied"))
            {
                warn!(error = %e, "Failed to confirm the path copy");
            }
        }));

    let prefs_pop = button.clone();
    state
        .handles
        .lock()
        .retain_signal(prefs.connect_clicked(move |_| {
            prefs_pop.popdown();
        }));

    SignalHeaderMenu {
        button,
        copy,
        prefs,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Context, Result, ensure},
        libadwaita::{
            gtk::{self, Window, test},
            prelude::{ButtonExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::signal_view::{
            signal_menu::build_signal_header_menu, signal_tab_build::build_signal_page,
        },
    };

    #[test]
    fn signal_header_menu_carries_copy_and_preferences() -> Result<()> {
        let state = Arc::new(AppState::mock().context("failed to build mock app state")?);
        let parent = Window::new();
        let tab = build_signal_page();
        let menu = build_signal_header_menu(&state, &parent, &tab);
        ensure!(
            menu.action_count() == 2,
            "signal menu must carry two actions"
        );
        ensure!(
            menu.copy_button().label().as_deref() == Some("Copy path summary"),
            "signal menu must carry the copy action"
        );
        ensure!(
            menu.prefs_button().tooltip_text().as_deref() == Some("Open preferences"),
            "signal menu must carry the preferences entry"
        );
        ensure!(
            [menu.copy_button(), menu.prefs_button()]
                .iter()
                .all(|button| button.can_focus()),
            "every action must be keyboard-reachable"
        );
        Ok(())
    }
}

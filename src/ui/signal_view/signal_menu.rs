//! Signal tab overflow menu: three read-only secondary actions.
//!
//! The sole allowed popover in the tab-only view (FR-014): a three-dot
//! `MenuButton` in the path header with exactly three keyboard-reachable
//! actions — copy the path summary (clipboard plus `Toast` confirmation, no
//! dialog), open output/device settings elsewhere (existing preferences
//! dialog on the audio page, no inline DSP editing), and app info (existing
//! preferences dialog default view). Owned by the tab page in
//! [`signal_tab`](crate::ui::signal_view::signal_tab).

use std::sync::Arc;

use {
    libadwaita::{
        gtk::{
            Box, Button, MenuButton, Orientation::Vertical, Popover, Window,
            accessible::Property::Label,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    ui::{
        preferences::{show_audio_preferences_dialog, show_preferences_dialog},
        signal_view::signal_tab::SignalTab,
    },
};

/// Overflow menu with the three read-only secondary actions.
#[derive(Debug, Clone)]
pub struct SignalMenu {
    /// Three-dot button in the path header owning the popover.
    button: MenuButton,
    /// Copies the path summary to the clipboard.
    copy: Button,
    /// Opens output/device settings on the audio page.
    settings: Button,
    /// Shows app info on the preferences default view.
    about: Button,
}

impl SignalMenu {
    /// Three-dot button for the path header bar.
    #[must_use]
    pub const fn menu_button(&self) -> &MenuButton {
        &self.button
    }

    /// Copy action button (test hook for the three-action contract).
    #[must_use]
    pub const fn copy_button(&self) -> &Button {
        &self.copy
    }

    /// Settings action button (test hook for the three-action contract).
    #[must_use]
    pub const fn settings_button(&self) -> &Button {
        &self.settings
    }

    /// About action button (test hook for the three-action contract).
    #[must_use]
    pub const fn about_button(&self) -> &Button {
        &self.about
    }

    /// Number of read-only actions parented in the popover (always three).
    #[must_use]
    pub fn action_count(&self) -> u32 {
        let parented = [&self.copy, &self.settings, &self.about]
            .iter()
            .filter(|button| button.parent().is_some())
            .count();
        u32::try_from(parented).unwrap_or(u32::MAX)
    }
}

/// Build one overflow action button with an accessible name.
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

/// Build the Signal tab overflow menu.
///
/// # Returns
///
/// * `SignalMenu` - Handles owning the `MenuButton` plus its three actions.
#[must_use]
pub fn build_signal_menu() -> SignalMenu {
    let copy = menu_action("Copy path summary");
    copy.set_tooltip_text(Some("Copy the signal path summary to the clipboard"));
    let settings = menu_action("Output settings");
    settings.set_tooltip_text(Some("Open output and device settings"));
    let about = menu_action("About");
    about.set_tooltip_text(Some("Show application information"));
    let list = Box::builder().orientation(Vertical).spacing(6).build();
    list.append(&copy);
    list.append(&settings);
    list.append(&about);
    let popover = Popover::builder().child(&list).has_arrow(true).build();
    let button = MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Signal path options")
        .can_focus(true)
        .popover(&popover)
        .build();
    button.update_property(&[Label("Signal path options")]);
    SignalMenu {
        button,
        copy,
        settings,
        about,
    }
}

/// Wire the three overflow actions for one Signal tab.
///
/// Copy writes the retained path summary to the clipboard and confirms with
/// a `Toast`; settings presents the existing preferences dialog on the audio
/// page; About presents the existing preferences default view. No dialog is
/// built in the tab and no DSP editing happens here.
///
/// # Arguments
///
/// * `tab` - Signal tab owning the menu and the retained summary.
/// * `state` - Application state for toasts and dialog settings.
/// * `parent` - Parent window used to present the preferences dialog.
pub fn wire_signal_menu(tab: &SignalTab, state: &Arc<AppState>, parent: &Window) {
    let copy_tab = tab.clone();
    let copy_menu = tab.menu().clone();
    let copy_toasts = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(tab.menu().copy_button().connect_clicked(move |_| {
            copy_menu.menu_button().popdown();
            let summary = copy_tab.pending_summary();
            copy_menu.menu_button().clipboard().set_text(&summary);
            if let Err(e) = copy_toasts
                .toast_tx
                .try_send(String::from("Signal path summary copied"))
            {
                warn!(error = %e, "Failed to confirm the path copy");
            }
        }));
    let settings_state = Arc::clone(state);
    let settings_window = parent.clone();
    let settings_menu = tab.menu().clone();
    state
        .handles
        .lock()
        .retain_signal(tab.menu().settings_button().connect_clicked(move |_| {
            settings_menu.menu_button().popdown();
            show_audio_preferences_dialog(&settings_state, &settings_window);
        }));
    let about_state = Arc::clone(state);
    let about_window = parent.clone();
    let about_menu = tab.menu().clone();
    state
        .handles
        .lock()
        .retain_signal(tab.menu().about_button().connect_clicked(move |_| {
            about_menu.menu_button().popdown();
            show_preferences_dialog(&about_state, &about_window);
        }));
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, test},
            prelude::{ButtonExt, WidgetExt},
        },
    };

    use crate::ui::signal_view::signal_menu::build_signal_menu;

    #[test]
    fn overflow_menu_carries_exactly_three_actions() -> Result<()> {
        let menu = build_signal_menu();
        ensure!(menu.action_count() == 3, "kebab must carry three actions");
        let labels = [
            menu.copy_button().label(),
            menu.settings_button().label(),
            menu.about_button().label(),
        ];
        ensure!(
            labels
                .iter()
                .map(|label| label.as_deref().unwrap_or(""))
                .collect::<Vec<_>>()
                == ["Copy path summary", "Output settings", "About"],
            "kebab must carry the read-only actions"
        );
        ensure!(
            [
                &menu.copy_button(),
                &menu.settings_button(),
                &menu.about_button()
            ]
            .iter()
            .all(|button| button.can_focus()),
            "every action must be keyboard-reachable"
        );
        Ok(())
    }
}

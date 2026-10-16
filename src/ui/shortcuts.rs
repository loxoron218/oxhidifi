//! Keyboard shortcuts dialog listing every application keybinding.
//!
//! Presents an `AdwShortcutsDialog` with General, Playback, Library, and
//! Navigation sections following GNOME HIG. The content is static: every
//! shortcut is always listed, with subtitles noting when it is ignored (the
//! `Signal` tab, pushed detail pages, or text-entry focus). Entry points are
//! the view-toggle popover, the signal menu, and `Ctrl+?` from anywhere; the
//! window wires the opener through the sibling
//! [`key_controllers`](crate::ui::key_controllers) module.

use std::sync::Arc;

use libadwaita::{
    ButtonContent,
    gdk::{Key, ModifierType},
    gtk::{Align::Start, Button, Window, accessible::Property::Label},
    prelude::{AccessibleExtManual, AdwDialogExt, ButtonExt},
};

use crate::{app::runtime::AppState, ui::shortcut_sections::build_shortcuts_dialog};

/// Build and present the keyboard shortcuts dialog.
///
/// # Arguments
///
/// * `parent` - Parent window used to present the dialog.
pub fn show_shortcuts_dialog(parent: &Window) {
    build_shortcuts_dialog().present(Some(parent));
}

/// Build the shared keyboard shortcuts entry button.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handles.
/// * `parent` - Parent window used to present the shortcuts dialog.
///
/// # Returns
///
/// * `Button` - Keyboard-focusable flat shortcuts button.
#[must_use]
pub fn build_shortcuts_button(state: &Arc<AppState>, parent: &Window) -> Button {
    let shortcuts_btn = Button::builder()
        .child(
            &ButtonContent::builder()
                .icon_name("preferences-desktop-keyboard-shortcuts-symbolic")
                .label("Keyboard Shortcuts")
                .halign(Start)
                .css_classes(["shortcuts-content"])
                .tooltip_text("Keyboard shortcuts content")
                .build(),
        )
        .tooltip_text("Open keyboard shortcuts (Ctrl+?)")
        .css_classes(["flat"])
        .hexpand(true)
        .build();
    shortcuts_btn.update_property(&[Label("Keyboard Shortcuts")]);
    let parent_shortcuts = parent.clone();
    state
        .handles
        .lock()
        .retain_signal(shortcuts_btn.connect_clicked(move |_| {
            show_shortcuts_dialog(&parent_shortcuts);
        }));
    shortcuts_btn
}

/// Whether the pressed key opens the shortcuts dialog (`Ctrl+?`).
///
/// Accepts `Ctrl+?` directly plus `Ctrl+Shift+/` for layouts emitting `slash`
/// instead of `question`, ignoring unrelated extra modifiers like the zoom
/// handler does. Intentionally works from text entries: `Ctrl+?` has no text
/// editing function to preserve, matching the `Ctrl+Tab` precedent.
///
/// # Arguments
///
/// * `key` - Pressed key.
/// * `modifiers` - Active modifiers.
///
/// # Returns
///
/// `true` when the dialog opener was pressed, `false` otherwise.
#[must_use]
pub fn is_shortcuts_key(key: Key, modifiers: ModifierType) -> bool {
    if !modifiers.intersects(ModifierType::CONTROL_MASK) {
        return false;
    }
    key == Key::question || (key == Key::slash && modifiers.intersects(ModifierType::SHIFT_MASK))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gdk::{Key, ModifierType},
            gtk::{self, Window, accessible::Property::Label, test},
            prelude::{AccessibleExtManual, ListModelExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::{
            shortcut_sections::{
                build_general_section, build_library_section, build_navigation_section,
                build_playback_section, build_shortcuts_dialog, shortcuts_item,
            },
            shortcuts::{build_shortcuts_button, is_shortcuts_key},
        },
    };

    #[test]
    fn shortcuts_item_carries_title_subtitle_accelerator() -> Result<()> {
        let item = shortcuts_item("Play / pause", "Only while shown", "space");
        ensure!(item.title().as_str() == "Play / pause");
        ensure!(item.subtitle().as_str() == "Only while shown");
        ensure!(item.accelerator().as_str() == "space");
        Ok(())
    }

    #[test]
    fn general_section_lists_five_items() -> Result<()> {
        let section = build_general_section();
        ensure!(
            section.title().as_deref() == Some("General"),
            "first section must be titled General"
        );
        ensure!(
            section.n_items() == 5,
            "General must list panel, tab, and opener shortcuts"
        );
        Ok(())
    }

    #[test]
    fn playback_section_lists_space_toggle() -> Result<()> {
        let section = build_playback_section();
        ensure!(
            section.title().as_deref() == Some("Playback"),
            "second section must be titled Playback"
        );
        ensure!(section.n_items() == 1, "Playback must list only play/pause");
        Ok(())
    }

    #[test]
    fn library_section_lists_zoom_and_view_toggle() -> Result<()> {
        let section = build_library_section();
        ensure!(
            section.title().as_deref() == Some("Library"),
            "third section must be titled Library"
        );
        ensure!(
            section.n_items() == 3,
            "Library must list zoom in, zoom out, and view toggle"
        );
        Ok(())
    }

    #[test]
    fn navigation_section_lists_open_and_back() -> Result<()> {
        let section = build_navigation_section();
        ensure!(
            section.title().as_deref() == Some("Navigation"),
            "fourth section must be titled Navigation"
        );
        ensure!(
            section.n_items() == 2,
            "Navigation must list card activation and going back"
        );
        Ok(())
    }

    #[test]
    fn build_shortcuts_dialog_starts_unmapped() -> Result<()> {
        let dialog = build_shortcuts_dialog();
        ensure!(
            !dialog.is_mapped(),
            "a fresh shortcuts dialog must not be mapped"
        );
        Ok(())
    }

    #[test]
    fn ctrl_question_opens_shortcuts() -> Result<()> {
        ensure!(
            is_shortcuts_key(Key::question, ModifierType::CONTROL_MASK),
            "Ctrl+? must open the shortcuts dialog"
        );
        ensure!(
            is_shortcuts_key(
                Key::question,
                ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK
            ),
            "Ctrl+Shift+? (Shift folded into the keyval) must open the dialog"
        );
        ensure!(
            is_shortcuts_key(
                Key::slash,
                ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK
            ),
            "Ctrl+Shift+/ must open the dialog on slash-emitting layouts"
        );
        Ok(())
    }

    #[test]
    fn shortcuts_key_ignores_unrelated_keys() -> Result<()> {
        ensure!(
            !is_shortcuts_key(Key::question, ModifierType::empty()),
            "? without Ctrl must not open the dialog"
        );
        ensure!(
            !is_shortcuts_key(Key::slash, ModifierType::CONTROL_MASK),
            "Ctrl+/ without Shift must not open the dialog"
        );
        ensure!(
            !is_shortcuts_key(Key::a, ModifierType::CONTROL_MASK),
            "an unrelated Ctrl key must not open the dialog"
        );
        Ok(())
    }

    #[test]
    fn build_shortcuts_button_is_keyboard_reachable() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let parent = Window::builder()
            .css_classes(["test-fixture", "background"])
            .tooltip_text("Test parent")
            .build();
        parent.update_property(&[Label("Test parent")]);
        let button = build_shortcuts_button(&state, &parent);
        ensure!(
            button.tooltip_text().as_deref() == Some("Open keyboard shortcuts (Ctrl+?)"),
            "shortcuts button must advertise its opener"
        );
        ensure!(
            button.can_focus(),
            "shortcuts button must be keyboard-reachable"
        );
        ensure!(
            button.hexpands(),
            "shortcuts button must fill the popover width"
        );
        ensure!(
            button.css_classes().contains(&"flat".into()),
            "shortcuts button must match the flat popover style"
        );
        Ok(())
    }
}

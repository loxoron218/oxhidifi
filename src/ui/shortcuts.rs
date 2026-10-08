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
    ButtonContent, ShortcutsDialog, ShortcutsItem, ShortcutsSection,
    gdk::{Key, ModifierType},
    gtk::{Button, Window, accessible::Property::Label},
    prelude::{AccessibleExtManual, AdwDialogExt, ButtonExt},
};

use crate::app::runtime::AppState;

/// Build one shortcuts item with a title, context subtitle, and accelerator.
///
/// # Arguments
///
/// * `title` - Short action description shown in the dialog.
/// * `subtitle` - Context note naming when the shortcut is ignored.
/// * `accelerator` - Accelerator string in `AdwShortcutLabel` format.
///
/// # Returns
///
/// * `ShortcutsItem` - Item carrying the title, subtitle, and accelerator.
fn shortcuts_item(title: &str, subtitle: &str, accelerator: &str) -> ShortcutsItem {
    let item = ShortcutsItem::new(title, accelerator);
    item.set_subtitle(subtitle);
    item
}

/// Build the General section: panel, tab cycling, and the dialog opener.
///
/// # Returns
///
/// * `ShortcutsSection` - General section with five items.
fn build_general_section() -> ShortcutsSection {
    let section = ShortcutsSection::new(Some("General"));
    section.add(shortcuts_item(
        "Toggle player panel",
        "Disabled while typing",
        "<Control>b",
    ));
    section.add(shortcuts_item(
        "Hide player panel",
        "Only while the panel is shown",
        "Escape",
    ));
    section.add(shortcuts_item(
        "Next library tab",
        "Albums, Artists, Signal; wraps around",
        "<Control>Tab",
    ));
    section.add(shortcuts_item(
        "Previous library tab",
        "Albums, Artists, Signal; wraps around",
        "<Control><Shift>Tab",
    ));
    section.add(shortcuts_item(
        "Keyboard shortcuts",
        "Open this dialog",
        "<Control>question",
    ));
    section
}

/// Build the Playback section: the play/pause toggle.
///
/// # Returns
///
/// * `ShortcutsSection` - Playback section with one item.
fn build_playback_section() -> ShortcutsSection {
    let section = ShortcutsSection::new(Some("Playback"));
    section.add(shortcuts_item(
        "Play / pause",
        "Only while the player panel is shown",
        "space",
    ));
    section
}

/// Build the Library section: zoom and grid/column toggle.
///
/// # Returns
///
/// * `ShortcutsSection` - Library section with three items.
fn build_library_section() -> ShortcutsSection {
    let section = ShortcutsSection::new(Some("Library"));
    section.add(shortcuts_item(
        "Zoom in",
        "Albums and Artists; numpad + works too",
        "<Control>plus",
    ));
    section.add(shortcuts_item(
        "Zoom out",
        "Albums and Artists; numpad − works too",
        "<Control>minus",
    ));
    section.add(shortcuts_item(
        "Toggle grid / column view",
        "Albums and Artists only",
        "<Control>g",
    ));
    section
}

/// Build the Navigation section: card activation and going back.
///
/// # Returns
///
/// * `ShortcutsSection` - Navigation section with two items.
fn build_navigation_section() -> ShortcutsSection {
    let section = ShortcutsSection::new(Some("Navigation"));
    section.add(shortcuts_item(
        "Open album or artist",
        "Enter or Space on a focused card",
        "Return",
    ));
    section.add(shortcuts_item(
        "Go back",
        "From album or artist detail pages",
        "Escape",
    ));
    section
}

/// Build the keyboard shortcuts dialog.
///
/// Assembles the General, Playback, Library, and Navigation sections into an
/// `AdwShortcutsDialog` per GNOME HIG: sections group related shortcuts and
/// every item carries an accelerator plus a context subtitle.
///
/// # Returns
///
/// * `ShortcutsDialog` - Dialog listing every application keybinding.
#[must_use]
pub fn build_shortcuts_dialog() -> ShortcutsDialog {
    let dialog = ShortcutsDialog::new();
    dialog.add(build_general_section());
    dialog.add(build_playback_section());
    dialog.add(build_library_section());
    dialog.add(build_navigation_section());
    dialog
}

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
                .build(),
        )
        .tooltip_text("Open keyboard shortcuts (Ctrl+?)")
        .css_classes(["flat"])
        .can_focus(true)
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
            gtk::{self, Window, test},
            prelude::{ListModelExt, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        ui::shortcuts::{
            build_general_section, build_library_section, build_navigation_section,
            build_playback_section, build_shortcuts_button, build_shortcuts_dialog,
            is_shortcuts_key, shortcuts_item,
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
        let parent = Window::new();
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

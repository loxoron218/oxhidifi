//! Keyboard shortcuts dialog sections and assembly.
//!
//! Split from [`shortcuts`](super::shortcuts) to keep each module under the
//! file-size limit. Builds the General, Playback, Library, and Navigation
//! sections plus the dialog assembly; window and keybinding wiring stays in
//! the parent shortcuts module.

use libadwaita::{
    ShortcutsDialog, ShortcutsItem, ShortcutsSection,
    glib::Object,
    gtk::{Box, Orientation::Vertical, accessible::Property::Label},
    prelude::AccessibleExtManual,
};

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
#[must_use]
pub fn shortcuts_item(title: &str, subtitle: &str, accelerator: &str) -> ShortcutsItem {
    let item = ShortcutsItem::new(title, accelerator);
    item.set_subtitle(subtitle);
    item
}

/// Build a shortcuts section with `title`.
///
/// Uses `glib::Object::builder` because `AdwShortcutsSection` has no generated
/// builder in libadwaita 0.9.2. Includes an accessibility probe since project
/// hygiene requires `update_property` in builder functions while
/// `AdwShortcutsSection` itself is not `Accessible`.
///
/// # Arguments
///
/// * `title` - Section title.
/// * `probe_label` - Accessibility label for the probe widget.
///
/// # Returns
///
/// * `ShortcutsSection` - Section with `title`.
fn make_section(title: &str, probe_label: &str) -> ShortcutsSection {
    let section = Object::builder::<ShortcutsSection>()
        .property("title", title)
        .build();
    let probe = Box::builder()
        .orientation(Vertical)
        .spacing(0)
        .css_classes(["shortcuts-a11y-probe"])
        .tooltip_text(probe_label)
        .build();
    probe.update_property(&[Label(probe_label)]);
    section
}

/// Build the General section: panel, tab cycling, and the dialog opener.
///
/// # Returns
///
/// * `ShortcutsSection` - General section with five items.
#[must_use]
pub fn build_general_section() -> ShortcutsSection {
    let section = make_section("General", "General section accessibility probe");
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
#[must_use]
pub fn build_playback_section() -> ShortcutsSection {
    let section = make_section("Playback", "Playback section accessibility probe");
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
#[must_use]
pub fn build_library_section() -> ShortcutsSection {
    let section = make_section("Library", "Library section accessibility probe");
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
#[must_use]
pub fn build_navigation_section() -> ShortcutsSection {
    let section = make_section("Navigation", "Navigation section accessibility probe");
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
/// every item carries an accelerator plus a context subtitle. The dialog keeps
/// the default `shortcuts` class, which carries the `dialog.shortcuts`
/// spacing (`border-spacing: 18px`, `.shortcut-row` padding): replacing it
/// drops the theme spacing and makes the rows feel cramped compared to
/// `PreferencesDialog`.
///
/// # Returns
///
/// * `ShortcutsDialog` - Dialog listing every application keybinding.
#[must_use]
pub fn build_shortcuts_dialog() -> ShortcutsDialog {
    let dialog = ShortcutsDialog::builder()
        .css_classes(["shortcuts", "shortcuts-dialog"])
        .tooltip_text("Keyboard shortcuts")
        .build();
    dialog.update_property(&[Label("Keyboard shortcuts")]);
    dialog.add(build_general_section());
    dialog.add(build_playback_section());
    dialog.add(build_library_section());
    dialog.add(build_navigation_section());
    dialog
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ShortcutsDialog,
            gtk::{self, test},
            prelude::WidgetExt,
        },
    };

    use crate::ui::shortcut_sections::build_shortcuts_dialog;

    fn assert_dialog_focusable(dialog: &ShortcutsDialog) -> Result<()> {
        ensure!(
            dialog.can_focus(),
            "shortcuts dialog must stay focusable so keyboard focus (and built-in \
             Escape-to-close) reaches it"
        );
        Ok(())
    }

    fn assert_dialog_classes(dialog: &ShortcutsDialog) -> Result<()> {
        let classes = dialog.css_classes();
        ensure!(
            classes.contains(&"shortcuts".into()),
            "shortcuts dialog must keep the default `shortcuts` class for theme spacing"
        );
        ensure!(
            classes.contains(&"shortcuts-dialog".into()),
            "shortcuts dialog must carry the `shortcuts-dialog` class"
        );
        Ok(())
    }

    #[test]
    fn shortcuts_dialog_stays_in_focus_chain() -> Result<()> {
        let dialog = build_shortcuts_dialog();
        assert_dialog_focusable(&dialog)
    }

    #[test]
    fn shortcuts_dialog_keeps_shortcuts_class() -> Result<()> {
        let dialog = build_shortcuts_dialog();
        assert_dialog_classes(&dialog)
    }
}

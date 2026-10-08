//! Text-input focus guard for global shortcuts.
//!
//! Typing in an entry must win over single-key shortcuts, so the window's
//! key controllers consult this helper before handling `Space` play/pause,
//! `Ctrl+B` panel toggle, or `Ctrl+G` view toggle.

use libadwaita::{
    glib::object::Cast,
    gtk::{Editable, TextView, Widget},
    prelude::{TextViewExt, WidgetExt},
};

/// Whether the focused widget (or any of its ancestors) accepts text input.
///
/// Walks up the widget ancestry so inner text children of an `Entry` still
/// count. Any [`Editable`] (`Entry`, `SearchEntry`, `PasswordEntry`,
/// `SpinButton`) counts, as does an editable [`TextView`]. A read-only
/// `TextView` does not count so `Space` can still toggle playback there.
///
/// # Arguments
///
/// * `focused` - Currently focused widget, if any.
///
/// # Returns
///
/// `true` when typing in the focused widget must win over the global
/// shortcuts.
pub fn focus_is_text_entry(focused: Option<&Widget>) -> bool {
    let mut current = focused.cloned();
    while let Some(widget) = current {
        if widget.dynamic_cast_ref::<Editable>().is_some() {
            return true;
        }
        if widget
            .dynamic_cast_ref::<TextView>()
            .is_some_and(TextViewExt::is_editable)
        {
            return true;
        }
        current = widget.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::object::Cast,
            gtk::{self, Entry, Label, SearchEntry, TextView, Widget, test as gtk_test},
            prelude::TextViewExt,
        },
    };

    use crate::ui::key_bindings::text_entry::focus_is_text_entry;

    #[test]
    fn focus_helper_signature_shape() {
        fn assert_shape<F: Fn(Option<&Widget>) -> bool>(_: F) {}
        assert_shape(focus_is_text_entry);
    }

    #[gtk_test]
    fn focus_none_is_not_text_entry() -> Result<()> {
        ensure!(
            !focus_is_text_entry(None),
            "no focus must not count as text input"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_plain_label_is_not_text_entry() -> Result<()> {
        let label = Label::new(Some("Title"));
        ensure!(
            !focus_is_text_entry(Some(label.upcast_ref())),
            "a label must not count as text input"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_entry_counts_as_text_entry() -> Result<()> {
        let entry = Entry::new();
        ensure!(
            focus_is_text_entry(Some(entry.upcast_ref())),
            "an entry must win over Space play/pause"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_search_entry_counts_as_text_entry() -> Result<()> {
        let entry = SearchEntry::new();
        ensure!(
            focus_is_text_entry(Some(entry.upcast_ref())),
            "a search entry must win over Space play/pause"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_editable_text_view_counts_as_text_entry() -> Result<()> {
        let view = TextView::new();
        view.set_editable(true);
        ensure!(
            focus_is_text_entry(Some(view.upcast_ref())),
            "an editable text view must win over Space play/pause"
        );
        Ok(())
    }

    #[gtk_test]
    fn focus_readonly_text_view_is_not_text_entry() -> Result<()> {
        let view = TextView::new();
        view.set_editable(false);
        ensure!(
            !focus_is_text_entry(Some(view.upcast_ref())),
            "a read-only text view must not block Space play/pause"
        );
        Ok(())
    }
}

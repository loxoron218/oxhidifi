//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use libadwaita::{
    HeaderBar as LibadwaitaHeaderBar, TabView,
    gtk::{Button, Label, ToggleButton, Widget},
    prelude::Cast,
};

/// Basic header bar with essential controls.
///
/// The `HeaderBar` provides a consistent interface for application
/// navigation, search functionality, and settings access.
pub struct HeaderBar {
    /// The underlying Libadwaita header bar widget.
    pub widget: LibadwaitaHeaderBar,
    /// Search toggle button.
    pub search_button: ToggleButton,
    /// View toggle button.
    pub view_toggle: ToggleButton,
    /// Settings button.
    pub settings_button: Button,
}

impl HeaderBar {
    /// Creates a new header bar instance.
    ///
    /// # Returns
    ///
    /// A new `HeaderBar` instance.
    pub fn new() -> Self {
        let widget = LibadwaitaHeaderBar::builder().build();

        // Search button
        let search_button = ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();
        widget.pack_start(&search_button);

        // View toggle button
        let view_toggle = ToggleButton::builder()
            .icon_name("view-grid-symbolic")
            .tooltip_text("Toggle View")
            .build();
        widget.pack_start(&view_toggle);

        // Settings button
        let settings_button = Button::builder()
            .icon_name("preferences-system-symbolic")
            .tooltip_text("Settings")
            .build();
        widget.pack_end(&settings_button);

        // Tab navigation
        let tab_view = TabView::builder().build();

        // TabPage doesn't have a new() constructor, create pages differently
        let albums_page = Label::new(Some("Albums"));
        tab_view.append(&albums_page.upcast::<Widget>());

        let artists_page = Label::new(Some("Artists"));
        tab_view.append(&artists_page.upcast::<Widget>());

        widget.set_title_widget(Some(&tab_view));

        Self {
            widget,
            search_button,
            view_toggle,
            settings_button,
        }
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::{init, prelude::ButtonExt};

    use crate::ui::header_bar::HeaderBar;

    #[test]
    fn test_header_bar_creation() {
        // Skip this test if we can't initialize GTK (e.g., in CI environments)
        if init().is_err() {
            return;
        }

        let header_bar = HeaderBar::new();

        // Check icon names without requiring widget realization
        assert_eq!(
            header_bar.search_button.icon_name().as_deref(),
            Some("system-search-symbolic")
        );
        assert_eq!(
            header_bar.view_toggle.icon_name().as_deref(),
            Some("view-grid-symbolic")
        );
        assert_eq!(
            header_bar.settings_button.icon_name().as_deref(),
            Some("preferences-system-symbolic")
        );
    }
}

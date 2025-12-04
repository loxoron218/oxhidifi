//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use libadwaita::gtk::prelude::*;

/// Basic header bar with essential controls.
///
/// The `HeaderBar` provides a consistent interface for application
/// navigation, search functionality, and settings access.
pub struct HeaderBar {
    /// The underlying Libadwaita header bar widget.
    pub widget: libadwaita::HeaderBar,
    /// Search toggle button.
    pub search_button: libadwaita::gtk::ToggleButton,
    /// View toggle button.
    pub view_toggle: libadwaita::gtk::ToggleButton,
    /// Settings button.
    pub settings_button: libadwaita::gtk::Button,
}

impl HeaderBar {
    /// Creates a new header bar instance.
    ///
    /// # Returns
    ///
    /// A new `HeaderBar` instance.
    pub fn new() -> Self {
        let widget = libadwaita::HeaderBar::builder().build();

        // Search button
        let search_button = libadwaita::gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();
        widget.pack_start(&search_button);

        // View toggle button
        let view_toggle = libadwaita::gtk::ToggleButton::builder()
            .icon_name("view-grid-symbolic")
            .tooltip_text("Toggle View")
            .build();
        widget.pack_start(&view_toggle);

        // Settings button
        let settings_button = libadwaita::gtk::Button::builder()
            .icon_name("preferences-system-symbolic")
            .tooltip_text("Settings")
            .build();
        widget.pack_end(&settings_button);

        // Tab navigation
        let tab_view = libadwaita::TabView::builder().build();
        
        // TabPage doesn't have a new() constructor, create pages differently
        let albums_page = libadwaita::gtk::Label::new(Some("Albums"));
        tab_view.append(&albums_page.upcast::<libadwaita::gtk::Widget>());
        
        let artists_page = libadwaita::gtk::Label::new(Some("Artists"));
        tab_view.append(&artists_page.upcast::<libadwaita::gtk::Widget>());
        
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
    use super::*;

    #[test]
    fn test_header_bar_creation() {
        // Skip this test if we can't initialize GTK (e.g., in CI environments)
        if libadwaita::gtk::init().is_err() {
            return;
        }
        
        let header_bar = HeaderBar::new();
        // Check icon names without requiring widget realization
        assert_eq!(header_bar.search_button.icon_name().as_deref(), Some("system-search-symbolic"));
        assert_eq!(header_bar.view_toggle.icon_name().as_deref(), Some("view-grid-symbolic"));
        assert_eq!(header_bar.settings_button.icon_name().as_deref(), Some("preferences-system-symbolic"));
    }
}
//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use gtk::prelude::*;
use libadwaita::prelude::*;

/// Basic header bar with essential controls.
///
/// The `HeaderBar` provides a consistent interface for application
/// navigation, search functionality, and settings access.
pub struct HeaderBar {
    /// The underlying Libadwaita header bar widget.
    pub widget: libadwaita::HeaderBar,
    /// Search toggle button.
    pub search_button: gtk::ToggleButton,
    /// View toggle button.
    pub view_toggle: gtk::ToggleButton,
    /// Settings button.
    pub settings_button: gtk::Button,
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
        let search_button = gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();
        widget.pack_start(&search_button);

        // View toggle button
        let view_toggle = gtk::ToggleButton::builder()
            .icon_name("view-grid-symbolic")
            .tooltip_text("Toggle View")
            .build();
        widget.pack_start(&view_toggle);

        // Settings button
        let settings_button = gtk::Button::builder()
            .icon_name("preferences-system-symbolic")
            .tooltip_text("Settings")
            .build();
        widget.pack_end(&settings_button);

        // Tab navigation
        let tab_view = libadwaita::TabView::builder().build();
        
        let albums_tab = libadwaita::TabPage::builder()
            .title("Albums")
            .build();
        tab_view.append(&albums_tab);
        
        let artists_tab = libadwaita::TabPage::builder()
            .title("Artists")
            .build();
        tab_view.append(&artists_tab);
        
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
        gtk::init().unwrap_or(());
        let header_bar = HeaderBar::new();
        assert!(header_bar.widget.is_valid());
        assert_eq!(header_bar.search_button.icon_name(), Some("system-search-symbolic"));
        assert_eq!(header_bar.view_toggle.icon_name(), Some("view-grid-symbolic"));
        assert_eq!(header_bar.settings_button.icon_name(), Some("preferences-system-symbolic"));
    }
}
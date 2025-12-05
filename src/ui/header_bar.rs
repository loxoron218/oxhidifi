//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use std::sync::Arc;

use libadwaita::{
    HeaderBar as LibadwaitaHeaderBar, TabView,
    gtk::{Button, Entry, Label, SearchBar, ToggleButton, Widget},
    prelude::{Cast, EditableExt, ObjectExt, ToggleButtonExt},
};

use crate::state::{
    AppState,
    ViewMode::{self, Grid, List},
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
    /// Search entry for expandable search.
    pub search_entry: Entry,
    /// Search bar container.
    pub search_bar: SearchBar,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Current view mode.
    pub current_view_mode: ViewMode,
}

impl HeaderBar {
    /// Creates a new header bar instance.
    ///
    /// # Returns
    ///
    /// A new `HeaderBar` instance.
    pub fn new(app_state: Option<Arc<AppState>>) -> Self {
        let widget = LibadwaitaHeaderBar::builder().build();

        // Create search entry
        let search_entry = Entry::builder()
            .placeholder_text("Search albums and artists...")
            .width_request(200)
            .build();

        // Create search bar
        let search_bar = SearchBar::builder()
            .search_mode_enabled(false)
            .show_close_button(true)
            .build();

        search_bar.set_child(Some(&search_entry));

        // Search button
        let search_button = ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();

        // Connect search button to search bar
        let search_bar_clone = search_bar.clone();
        search_button.connect_toggled(move |button: &ToggleButton| {
            search_bar_clone.set_search_mode(button.is_active());
            if button.is_active() {
                search_bar_clone.set_search_mode(true);
            }
        });

        // Connect search entry to app state
        if let Some(ref state) = app_state {
            let state_clone = state.clone();
            search_entry.connect_changed(move |entry: &Entry| {
                let text = entry.text().to_string();
                if text.is_empty() {
                    state_clone.update_search_filter(None);
                } else {
                    state_clone.update_search_filter(Some(text));
                }
            });
        }

        widget.pack_start(&search_button);

        // View toggle button
        let current_view_mode = app_state
            .as_ref()
            .map(|s| s.get_library_state().view_mode)
            .unwrap_or(Grid);

        let view_toggle_icon = match current_view_mode {
            List => "view-list-symbolic",
            Grid => "view-grid-symbolic",
        };

        let view_toggle = ToggleButton::builder()
            .icon_name(view_toggle_icon)
            .tooltip_text("Toggle View")
            .active(current_view_mode == List)
            .build();

        // Connect view toggle to app state
        if let Some(ref state) = app_state {
            let state_clone = state.clone();
            let view_toggle_clone = view_toggle.clone();
            view_toggle.connect_toggled(move |button: &ToggleButton| {
                let new_mode = if button.is_active() { List } else { Grid };

                // Update icon
                let icon_name = if button.is_active() {
                    "view-list-symbolic"
                } else {
                    "view-grid-symbolic"
                };
                view_toggle_clone.set_property("icon-name", icon_name);

                // Update app state
                let mut library_state = state_clone.get_library_state();
                library_state.view_mode = new_mode;
                state_clone.update_library_state(library_state);
            });
        }

        widget.pack_start(&view_toggle);

        // Settings button
        let settings_button = Button::builder()
            .icon_name("preferences-system-symbolic")
            .tooltip_text("Settings")
            .build();
        widget.pack_end(&settings_button);

        // Tab navigation with proper TabView
        let tab_view = TabView::builder().build();

        let albums_page = Label::new(Some("Albums"));
        tab_view.append(&albums_page.upcast::<Widget>());

        let artists_page = Label::new(Some("Artists"));
        tab_view.append(&artists_page.upcast::<Widget>());

        // Set tab titles properly
        tab_view.nth_page(0).set_title("Albums");
        tab_view.nth_page(1).set_title("Artists");
        widget.set_title_widget(Some(&tab_view));

        // Connect tab view to app state for navigation
        if let Some(ref state) = app_state {
            let _state_clone = state.clone();
            tab_view.connect_selected_page_notify(move |tab_view| {
                if let Some(selected_page) = tab_view.selected_page() {
                    let page_index = tab_view.page_position(&selected_page);

                    // This would trigger view-specific updates in a real implementation
                    // For now, we just log the selection
                    println!("Selected tab: {}", page_index);
                }
            });
        }

        Self {
            widget,
            search_button,
            view_toggle,
            settings_button,
            search_entry,
            search_bar,
            app_state,
            current_view_mode,
        }
    }
}

impl HeaderBar {
    /// Creates a header bar with default configuration.
    pub fn default_with_state(app_state: Arc<AppState>) -> Self {
        Self::new(Some(app_state))
    }
}

// Remove the Default impl since it requires AppState

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

        let header_bar = HeaderBar::new(None);

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

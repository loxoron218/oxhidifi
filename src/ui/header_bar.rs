//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use std::sync::Arc;

use libadwaita::{
    HeaderBar as LibadwaitaHeaderBar,
    glib::MainContext,
    gtk::{Box as GtkBox, Button, Entry, Orientation::Horizontal, SearchBar, ToggleButton},
    prelude::{BoxExt, EditableExt, ObjectExt, ToggleButtonExt},
};

use crate::state::{
    AppState,
    AppStateEvent::{self, LibraryStateChanged},
    ViewMode::{self, Grid, List},
    app_state::LibraryTab::{Albums, Artists},
};

/// Adaptive header bar with search, navigation, and action controls.
///
/// The `HeaderBar` provides a consistent interface for application
/// navigation, search functionality, settings access, and album/artist tab navigation.
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
    /// Album tab button.
    pub album_tab: ToggleButton,
    /// Artist tab button.
    pub artist_tab: ToggleButton,
    /// Tab container box.
    pub tab_box: GtkBox,
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

            // Subscribe to state changes to update view toggle
            let view_toggle_update = view_toggle.clone();
            let state_clone_update = state.clone();

            MainContext::default().spawn_local(async move {
                let mut receiver = state_clone_update.subscribe();
                while let Ok(event) = receiver.recv().await {
                    if let LibraryStateChanged(new_state) = event {
                        // Update view toggle state based on current view mode
                        let is_list = new_state.view_mode == List;
                        view_toggle_update.set_active(is_list);

                        // Update icon
                        let icon_name = if is_list {
                            "view-list-symbolic"
                        } else {
                            "view-grid-symbolic"
                        };
                        view_toggle_update.set_property("icon-name", icon_name);
                    }
                }
            });
        }

        widget.pack_start(&view_toggle);

        // Settings button
        let settings_button = Button::builder()
            .icon_name("preferences-system-symbolic")
            .tooltip_text("Settings")
            .build();
        widget.pack_end(&settings_button);

        // Create tab navigation buttons for Albums/Artists
        let current_tab = app_state
            .as_ref()
            .map(|s| s.get_library_state().current_tab)
            .unwrap_or(Albums);

        let album_tab = ToggleButton::builder()
            .label("Albums")
            .tooltip_text("Browse albums")
            .active(current_tab == Albums)
            .build();

        let artist_tab = ToggleButton::builder()
            .label("Artists")
            .tooltip_text("Browse artists")
            .active(current_tab == Artists)
            .build();

        // Set up mutual exclusivity for tab buttons
        artist_tab.set_group(Some(&album_tab));

        // Connect tab buttons to app state
        if let Some(ref state) = app_state {
            let state_clone_album = state.clone();
            let state_clone_artist = state.clone();
            let artist_tab_clone = artist_tab.clone();
            let album_tab_clone = album_tab.clone();

            album_tab.connect_toggled(move |button: &ToggleButton| {
                // Only process if this button is being activated (not deactivated)
                if button.is_active() {
                    // Update app state
                    let mut library_state = state_clone_album.get_library_state();
                    library_state.current_tab = Albums;
                    state_clone_album.update_library_state(library_state);

                    // Ensure artist tab is not active
                    artist_tab_clone.set_active(false);
                }
            });

            artist_tab.connect_toggled(move |button: &ToggleButton| {
                // Only process if this button is being activated (not deactivated)
                if button.is_active() {
                    // Update app state
                    let mut library_state = state_clone_artist.get_library_state();
                    library_state.current_tab = Artists;
                    state_clone_artist.update_library_state(library_state);

                    // Ensure album tab is not active
                    album_tab_clone.set_active(false);
                }
            });

            // Subscribe to state changes to update tab buttons
            let album_tab_update = album_tab.clone();
            let artist_tab_update = artist_tab.clone();
            let state_clone_update = state.clone();

            MainContext::default().spawn_local(async move {
                let mut receiver = state_clone_update.subscribe();
                while let Ok(event) = receiver.recv().await {
                    if let AppStateEvent::LibraryStateChanged(new_state) = event {
                        // Update tab button states based on current tab
                        album_tab_update.set_active(new_state.current_tab == Albums);
                        artist_tab_update.set_active(new_state.current_tab == Artists);
                    }
                }
            });
        }

        // Create tab container box
        let tab_box = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(6)
            .css_classes(["linked"])
            .build();

        tab_box.append(&album_tab);
        tab_box.append(&artist_tab);

        widget.set_title_widget(Some(&tab_box));

        Self {
            widget,
            search_button,
            view_toggle,
            settings_button,
            search_entry,
            search_bar,
            album_tab,
            artist_tab,
            tab_box,
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
    use libadwaita::prelude::ButtonExt;

    use crate::ui::header_bar::HeaderBar;

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_header_bar_creation() {
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

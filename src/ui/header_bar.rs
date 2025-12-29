//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use std::sync::Arc;

use {
    libadwaita::{
        HeaderBar as LibadwaitaHeaderBar, SplitButton,
        gio::{Icon, Menu, MenuItem, SimpleAction, SimpleActionGroup},
        glib::{JoinHandle, MainContext, Variant, VariantTy},
        gtk::{
            Box, Button, Entry, Image, Label,
            Orientation::{Horizontal, Vertical},
            Popover, SearchBar, Separator, ToggleButton,
        },
        prelude::{ActionMapExt, BoxExt, ButtonExt, EditableExt, ToggleButtonExt, WidgetExt},
    },
    tracing::{debug, info},
};

use crate::state::{
    AppState,
    AppStateEvent::ViewOptionsChanged,
    NavigationState::Library,
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
    /// View split button.
    pub view_split_button: SplitButton,
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
    pub tab_box: Box,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Current view mode.
    pub current_view_mode: ViewMode,
    /// Back button for detail views.
    pub back_button: Button,
    /// Zoom out button for popover.
    pub zoom_out_button: Button,
    /// Zoom in button for popover.
    pub zoom_in_button: Button,
    /// Zoom popover container.
    pub zoom_popover: Popover,
    /// Subscription handle for state changes (to ensure proper cleanup)
    _subscription_handle: Option<JoinHandle<()>>,
}

impl HeaderBar {
    /// Creates a new header bar instance.
    ///
    /// # Returns
    ///
    /// A new `HeaderBar` instance.
    pub fn new(app_state: Option<Arc<AppState>>) -> Self {
        let widget = LibadwaitaHeaderBar::builder().build();

        // Create back button
        let back_button = Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back")
            .visible(false) // Hidden by default
            .build();

        // Connect back button to app state
        if let Some(ref state) = app_state {
            let state_clone = state.clone();
            back_button.connect_clicked(move |_| {
                // Navigate back to library root
                state_clone.update_navigation(Library);
            });
        }

        widget.pack_start(&back_button);

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

        // View split button
        let current_view_mode = app_state
            .as_ref()
            .map(|s| s.get_library_state().view_mode)
            .unwrap_or(Grid);

        let view_button_icon = match current_view_mode {
            List => "view-list-symbolic",
            Grid => "view-grid-symbolic",
        };

        // Create menu for dropdown
        let menu = Menu::new();

        // Add Grid view option
        let grid_item = MenuItem::new(Some("Grid View"), Some("view.set-mode"));
        grid_item.set_attribute_value("target", Some(&Variant::from(Grid as i32)));
        if let Ok(icon) = Icon::for_string("view-grid-symbolic") {
            grid_item.set_icon(&icon);
        }
        menu.append_item(&grid_item);

        // Add List view option
        let list_item = MenuItem::new(Some("List View"), Some("view.set-mode"));
        list_item.set_attribute_value("target", Some(&Variant::from(List as i32)));
        if let Ok(icon) = Icon::for_string("view-list-symbolic") {
            list_item.set_icon(&icon);
        }
        menu.append_item(&list_item);

        let view_split_button = SplitButton::builder()
            .icon_name(view_button_icon)
            .tooltip_text("Toggle View")
            .menu_model(&menu)
            .build();

        // Create zoom popover content
        let zoom_box = Box::builder()
            .orientation(Vertical) // Changed to Vertical as per requirements
            .spacing(6)
            .build();

        // Create main horizontal container for label and zoom buttons
        let zoom_controls_box = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        // Add "Icon Size" label
        let icon_size_label = Label::builder().label("Icon Size").build();
        zoom_controls_box.append(&icon_size_label);

        // Create zoom buttons container (horizontal pill)
        let zoom_buttons_box = Box::builder()
            .orientation(Horizontal)
            .spacing(0)
            .css_classes(["linked", "flat"])
            .build();

        // Create zoom buttons
        let zoom_out_button = Button::builder()
            .icon_name("zoom-out-symbolic")
            .tooltip_text("Zoom Out")
            .css_classes(["flat"])
            .build();

        let zoom_in_button = Button::builder()
            .icon_name("zoom-in-symbolic")
            .tooltip_text("Zoom In")
            .css_classes(["flat"])
            .build();

        zoom_buttons_box.append(&zoom_out_button);
        zoom_buttons_box.append(&zoom_in_button);

        zoom_controls_box.append(&zoom_buttons_box);
        zoom_box.append(&zoom_controls_box);

        // Add separator after zoom controls
        let separator = Separator::new(Horizontal);
        zoom_box.append(&separator);

        // Create popover
        let zoom_popover = Popover::builder().child(&zoom_box).has_arrow(true).build();

        // Set popover on the split button's arrow
        view_split_button.set_popover(Some(&zoom_popover));

        // Connect main button click to toggle view mode
        if let Some(ref state) = app_state {
            let state_clone_main = state.clone();
            let view_split_button_clone_main = view_split_button.clone();

            // Main button click handler - toggles between current mode and the other mode
            view_split_button.connect_clicked(move |_| {
                let current_state = state_clone_main.get_library_state();
                let new_mode = if current_state.view_mode == Grid {
                    List
                } else {
                    Grid
                };

                // Check if state actually changed
                if current_state.view_mode == new_mode {
                    debug!("View mode unchanged, skipping update");
                    return;
                }

                info!("View mode toggled to: {:?}", new_mode);

                // Update icon
                let icon_name = match new_mode {
                    List => "view-list-symbolic",
                    Grid => "view-grid-symbolic",
                };
                view_split_button_clone_main.set_icon_name(icon_name);

                // Update app state
                state_clone_main.update_view_options(current_state.current_tab, new_mode);
            });

            // Connect menu actions to app state
            let state_clone_menu = state.clone();
            let view_split_button_clone_menu = view_split_button.clone();

            // Handle set-mode action (menu item clicks)
            let set_mode_action = SimpleAction::new("view.set-mode", Some(VariantTy::INT32));
            let state_clone_set = state_clone_menu.clone();
            let view_split_button_clone_set = view_split_button_clone_menu.clone();

            set_mode_action.connect_activate(move |_action, parameter: Option<&Variant>| {
                if let Some(param) = parameter {
                    let mode_value = param.get::<i32>().unwrap();
                    let new_mode = match mode_value {
                        0 => Grid, // Grid = 0
                        1 => List, // List = 1
                        _ => return,
                    };

                    // Check if state actually changed
                    let current_state = state_clone_set.get_library_state();
                    if current_state.view_mode == new_mode {
                        debug!("View mode unchanged, skipping update");
                        return;
                    }

                    info!("View mode changed to: {:?}", new_mode);

                    // Update icon
                    let icon_name = match new_mode {
                        List => "view-list-symbolic",
                        Grid => "view-grid-symbolic",
                    };
                    view_split_button_clone_set.set_icon_name(icon_name);

                    // Update app state
                    state_clone_set.update_view_options(current_state.current_tab, new_mode);
                }
            });

            // Add action to the widget itself since we can't easily access parent action groups
            let action_group = SimpleActionGroup::new();
            action_group.add_action(&set_mode_action);
            view_split_button.insert_action_group("win", Some(&action_group));
        }

        // Connect zoom buttons to app state if available
        if let Some(ref state) = app_state {
            let state_clone_zoom_out = state.clone();
            let state_clone_zoom_in = state.clone();

            // Zoom out handler
            zoom_out_button.connect_clicked(move |_| {
                let current_view_mode = state_clone_zoom_out.get_library_state().view_mode;
                match current_view_mode {
                    Grid => {
                        state_clone_zoom_out.decrease_grid_zoom_level();
                    }
                    List => {
                        state_clone_zoom_out.decrease_list_zoom_level();
                    }
                }
            });

            // Zoom in handler
            zoom_in_button.connect_clicked(move |_| {
                let current_view_mode = state_clone_zoom_in.get_library_state().view_mode;
                match current_view_mode {
                    Grid => {
                        state_clone_zoom_in.increase_grid_zoom_level();
                    }
                    List => {
                        state_clone_zoom_in.increase_list_zoom_level();
                    }
                }
            });
        }

        // Settings button
        let settings_button = Button::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Settings")
            .build();

        // Pack settings button first (will appear on far right)
        widget.pack_end(&settings_button);

        // Then pack view split button (will appear immediately to left of settings)
        widget.pack_end(&view_split_button);

        // Create tab navigation buttons for Albums/Artists
        let current_tab = app_state
            .as_ref()
            .map(|s| s.get_library_state().current_tab)
            .unwrap_or(Albums);

        // Create Albums tab with both icon and text
        let album_icon = Image::builder().icon_name("folder-music-symbolic").build();
        let album_label = Label::builder().label("Albums").build();
        let album_box = Box::builder().orientation(Horizontal).spacing(6).build();
        album_box.append(&album_icon);
        album_box.append(&album_label);

        let album_tab = ToggleButton::builder()
            .child(&album_box)
            .tooltip_text("Browse albums")
            .active(current_tab == Albums)
            .has_frame(false)
            .build();

        // Create Artists tab with both icon and text
        let artist_icon = Image::builder()
            .icon_name("avatar-default-symbolic")
            .build();
        let artist_label = Label::builder().label("Artists").build();
        let artist_box = Box::builder().orientation(Horizontal).spacing(6).build();
        artist_box.append(&artist_icon);
        artist_box.append(&artist_label);

        let artist_tab = ToggleButton::builder()
            .child(&artist_box)
            .tooltip_text("Browse artists")
            .active(current_tab == Artists)
            .has_frame(false)
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
                    // Check if state actually changed
                    let current_state = state_clone_album.get_library_state();
                    if current_state.current_tab == Albums {
                        debug!("Album tab already active, skipping update");
                        return;
                    }

                    info!("Switching to Albums tab");

                    // Update app state using lightweight navigation update
                    state_clone_album.update_view_options(Albums, current_state.view_mode);

                    // Ensure artist tab is not active
                    artist_tab_clone.set_active(false);
                }
            });

            artist_tab.connect_toggled(move |button: &ToggleButton| {
                // Only process if this button is being activated (not deactivated)
                if button.is_active() {
                    // Check if state actually changed
                    let current_state = state_clone_artist.get_library_state();
                    if current_state.current_tab == Artists {
                        debug!("Artist tab already active, skipping update");
                        return;
                    }

                    info!("Switching to Artists tab");

                    // Update app state using lightweight navigation update
                    state_clone_artist.update_view_options(Artists, current_state.view_mode);

                    // Ensure album tab is not active
                    album_tab_clone.set_active(false);
                }
            });
        }

        // Create tab container box
        let tab_box = Box::builder().orientation(Horizontal).spacing(6).build();

        tab_box.append(&album_tab);
        tab_box.append(&artist_tab);

        widget.set_title_widget(Some(&tab_box));

        Self {
            widget,
            search_button,
            view_split_button: view_split_button.clone(),
            settings_button,
            search_entry,
            search_bar,
            album_tab,
            artist_tab,
            tab_box,
            back_button,
            zoom_out_button,
            zoom_in_button,
            zoom_popover,
            app_state: app_state.clone(),
            current_view_mode: current_view_mode.clone(),
            _subscription_handle: if let Some(ref state) = app_state {
                // Create subscription handle for state changes
                let state_clone_sub = state.clone();
                let view_split_button_clone_sub = view_split_button.clone();
                let handle = MainContext::default().spawn_local(async move {
                    let rx = state_clone_sub.subscribe();
                    while let Ok(event) = rx.recv().await {
                        if let ViewOptionsChanged { view_mode, .. } = event {
                            // Update icon based on new view mode
                            let icon_name = match view_mode {
                                List => "view-list-symbolic",
                                Grid => "view-grid-symbolic",
                            };
                            view_split_button_clone_sub.set_icon_name(icon_name);
                        }
                    }
                });
                Some(handle)
            } else {
                None
            },
        }
    }
}

impl HeaderBar {
    /// Creates a header bar with default configuration.
    pub fn default_with_state(app_state: Arc<AppState>) -> Self {
        Self::new(Some(app_state))
    }
}

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
            header_bar.view_split_button.icon_name().as_deref(),
            Some("view-grid-symbolic")
        );
        assert_eq!(
            header_bar.settings_button.icon_name().as_deref(),
            Some("open-menu-symbolic")
        );
    }
}

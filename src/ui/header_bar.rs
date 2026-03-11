//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.

use std::{sync::Arc, time::Duration};

use {
    libadwaita::{
        Application, ApplicationWindow, HeaderBar as LibadwaitaHeaderBar, SplitButton,
        gio::{Icon, Menu, MenuItem, SimpleAction, SimpleActionGroup},
        glib::{JoinHandle, MainContext, SourceId, Variant, VariantTy, timeout_add_local_once},
        gtk::{
            Box, Button, Image, Label,
            Orientation::{Horizontal, Vertical},
            Popover, SearchEntry, Separator, ToggleButton,
        },
        prelude::{
            ActionMapExt, BoxExt, ButtonExt, Cast, EditableExt, GtkApplicationExt, ToggleButtonExt,
            WidgetExt,
        },
    },
    parking_lot::Mutex,
    tracing::{debug, error, info, warn},
};

use crate::{
    config::settings::SettingsManager,
    library::database::LibraryDatabase,
    state::app_state::{
        AppState,
        AppStateEvent::ViewOptionsChanged,
        LibraryTab::{Albums, Artists},
        NavigationState::Library,
        ViewMode::{self, Grid, List},
    },
    ui::preferences::dialog::PreferencesDialog,
};

/// Type alias for search debounce timer handle.
type SearchDebounceHandle = Arc<Mutex<Option<SourceId>>>;

/// Type alias for search clearing flag.
type SearchClearingFlag = Arc<Mutex<bool>>;

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
    /// Application reference for preferences dialog.
    pub application: Option<Arc<Application>>,
    /// Search entry for inline search.
    pub search_entry: SearchEntry,
    /// Album tab button.
    pub album_tab: ToggleButton,
    /// Artist tab button.
    pub artist_tab: ToggleButton,
    /// Tab container box.
    pub tab_box: Box,
    /// Application state reference.
    pub app_state: Arc<AppState>,
    /// Settings manager reference.
    pub settings_manager: Arc<SettingsManager>,
    /// Current view mode.
    pub current_view_mode: ViewMode,
    /// Library database reference for preferences dialog.
    pub library_db: Option<Arc<LibraryDatabase>>,
    /// Back button for detail views.
    pub back_button: Button,
    /// Zoom out button for popover.
    pub zoom_out_button: Button,
    /// Zoom in button for popover.
    pub zoom_in_button: Button,
    /// Zoom popover container.
    pub zoom_popover: Popover,
    /// Subscription handle for state changes (to ensure proper cleanup)
    _subscription_handle: JoinHandle<()>,
    /// Debounce timer handle for search input.
    _search_debounce_handle: Arc<Mutex<Option<SourceId>>>,
    /// Flag to prevent search debounce during programmatic text clearing.
    clearing_search: Arc<Mutex<bool>>,
}

impl HeaderBar {
    /// Creates a new header bar instance.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `application` - Optional application reference for dialog parent
    /// * `settings_manager` - Settings manager reference
    /// * `library_db` - Optional library database reference for preferences dialog
    ///
    /// # Returns
    ///
    /// A new `HeaderBar` instance.
    pub fn new(
        app_state: &Arc<AppState>,
        application: Option<Application>,
        settings_manager: Arc<SettingsManager>,
        library_db: Option<Arc<LibraryDatabase>>,
    ) -> Self {
        let widget = LibadwaitaHeaderBar::builder().build();

        let back_button = Self::create_back_button(app_state);
        widget.pack_start(&back_button);

        let current_view_mode = app_state.get_library_state().view_mode;
        let (search_button, search_entry, debounce_handle, clearing_search) =
            Self::setup_search_functionality(app_state, &settings_manager);

        widget.pack_start(&search_button);
        widget.pack_start(&search_entry);

        let menu = Self::create_view_menu();

        let view_split_button = SplitButton::builder()
            .icon_name(Self::get_view_icon_name(&current_view_mode))
            .tooltip_text("Toggle View")
            .menu_model(&menu)
            .build();

        let (zoom_popover, zoom_out_button, zoom_in_button) = Self::create_zoom_popover();
        view_split_button.set_popover(Some(&zoom_popover));

        Self::connect_view_button_handlers(app_state, &view_split_button);

        Self::connect_zoom_button_handlers(app_state, &zoom_out_button, &zoom_in_button);

        let application_arc = application.map(Arc::new);
        let settings_button =
            Self::create_settings_button(app_state, application_arc.as_ref(), library_db.as_ref());
        widget.pack_end(&settings_button);
        widget.pack_end(&view_split_button);

        let (album_tab, artist_tab, tab_box) = Self::create_tab_buttons(app_state);
        widget.set_title_widget(Some(&tab_box));

        let subscription_handle = Self::subscribe_to_view_options(app_state, &view_split_button);

        Self {
            widget,
            search_button,
            view_split_button,
            settings_button,
            search_entry,
            album_tab,
            artist_tab,
            tab_box,
            back_button,
            zoom_out_button,
            zoom_in_button,
            zoom_popover,
            app_state: app_state.clone(),
            settings_manager,
            application: application_arc,
            current_view_mode,
            library_db,
            _search_debounce_handle: debounce_handle,
            clearing_search,
            _subscription_handle: subscription_handle,
        }
    }

    /// Creates and configures the back button.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for navigation updates
    ///
    /// # Returns
    ///
    /// Configured back button widget.
    fn create_back_button(app_state: &Arc<AppState>) -> Button {
        let back_button = Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back")
            .use_underline(true)
            .visible(false)
            .build();

        // Connect back button to app state
        let state_clone = app_state.clone();
        back_button.connect_clicked(move |_| {
            // Navigate back to library root
            state_clone.update_navigation(Library);
        });

        back_button
    }

    /// Creates search entry, button, and connects debounced search functionality.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `settings_manager` - Settings manager for debounce configuration
    ///
    /// # Returns
    ///
    /// Tuple of (`search_button`, `search_entry`, `debounce_handle`, `clearing_search_flag`).
    fn setup_search_functionality(
        app_state: &Arc<AppState>,
        settings_manager: &Arc<SettingsManager>,
    ) -> (
        ToggleButton,
        SearchEntry,
        SearchDebounceHandle,
        SearchClearingFlag,
    ) {
        let search_entry = SearchEntry::builder()
            .placeholder_text("Search albums and artists...")
            .width_request(200)
            .visible(false)
            .build();

        // Search button
        let search_button = ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .use_underline(true)
            .build();

        // Debounce timer handle for search
        let debounce_handle: Arc<Mutex<Option<SourceId>>> = Arc::new(Mutex::new(None));

        // Flag to prevent debounce during programmatic text clearing
        let clearing_search: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        // Connect search button to toggle entry visibility and focus
        let search_entry_clone = search_entry.clone();
        let debounce_btn = debounce_handle.clone();
        let clearing_btn = clearing_search.clone();
        search_button.connect_toggled(move |button: &ToggleButton| {
            search_entry_clone.set_visible(button.is_active());
            if button.is_active() {
                search_entry_clone.grab_focus();
            } else {
                *clearing_btn.lock() = true;
                search_entry_clone.set_text("");
                *clearing_btn.lock() = false;

                // Cancel any pending debounce timer
                if let Some(timer_id) = debounce_btn.lock().take() {
                    let () = timer_id.remove();
                }
            }
        });

        // Handle Escape to hide search entry
        let search_button_clone = search_button.clone();
        let debounce_esc = debounce_handle.clone();
        let search_entry_clone_escape = search_entry.clone();
        let state_esc = app_state.clone();
        let clearing_esc = clearing_search.clone();
        search_entry.connect_stop_search(move |_| {
            search_button_clone.set_active(false);

            // Clear search text and filter when ESC is pressed
            *clearing_esc.lock() = true;
            search_entry_clone_escape.set_text("");
            *clearing_esc.lock() = false;

            // Cancel any pending debounce timer
            if let Some(timer_id) = debounce_esc.lock().take() {
                let () = timer_id.remove();
            }

            // Reset search filter
            state_esc.update_search_filter(None);
        });

        // Connect search entry to app state with debouncing
        let state_clone = app_state.clone();
        let debounce_search = debounce_handle.clone();
        let clearing_ch = clearing_search.clone();
        let settings_manager_search = settings_manager.clone();
        search_entry.connect_search_changed(move |entry| {
            // Skip debounce during programmatic text clearing
            if *clearing_ch.lock() {
                return;
            }

            let text = entry.text().to_string();

            // Cancel any pending debounce timer
            if let Some(timer_id) = debounce_search.lock().take() {
                let () = timer_id.remove();
            }

            let state = state_clone.clone();

            // Update immediately if empty, otherwise debounce
            if text.is_empty() {
                state.update_search_filter(None);
            } else {
                let handle_clone = debounce_search.clone();
                let handle_clone_for_id = debounce_search.clone();

                let debounce_ms = settings_manager_search.get_settings().search_debounce_ms;

                let timer_id =
                    timeout_add_local_once(Duration::from_millis(debounce_ms), move || {
                        state.update_search_filter(Some(text));

                        // Clear timer ID after execution since it's already been removed by glib
                        *handle_clone.lock() = None;
                    });

                *handle_clone_for_id.lock() = Some(timer_id);
            }
        });

        (
            search_button,
            search_entry,
            debounce_handle,
            clearing_search,
        )
    }

    /// Creates the view mode menu with Grid and List options.
    ///
    /// # Returns
    ///
    /// Configured menu widget.
    fn create_view_menu() -> Menu {
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

        menu
    }

    /// Creates the zoom popover with zoom in/out controls.
    ///
    /// # Returns
    ///
    /// Tuple of (`zoom_popover`, `zoom_out_button`, `zoom_in_button`).
    fn create_zoom_popover() -> (Popover, Button, Button) {
        let zoom_box = Box::builder().orientation(Vertical).spacing(6).build();

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
            .css_classes(["linked", "flat"])
            .build();

        // Create zoom buttons
        let zoom_out_button = Button::builder()
            .icon_name("zoom-out-symbolic")
            .tooltip_text("Zoom Out")
            .use_underline(true)
            .css_classes(["flat"])
            .build();

        let zoom_in_button = Button::builder()
            .icon_name("zoom-in-symbolic")
            .tooltip_text("Zoom In")
            .use_underline(true)
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

        (zoom_popover, zoom_out_button, zoom_in_button)
    }

    /// Connects view button handlers for mode toggling and zoom controls.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `view_split_button` - View split button widget
    fn connect_view_button_handlers(app_state: &Arc<AppState>, view_split_button: &SplitButton) {
        // Connect main button click to toggle view mode
        let state_clone_main = app_state.clone();
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

            debug!("View mode toggled to: {:?}", new_mode);

            // Update icon
            let icon_name = Self::get_view_icon_name(&new_mode);
            view_split_button_clone_main.set_icon_name(icon_name);

            // Update app state
            state_clone_main.update_view_options(current_state.current_tab, new_mode);
        });

        // Connect menu actions to app state
        let set_mode_action = SimpleAction::new("view.set-mode", Some(VariantTy::INT32));
        let state_clone_set = app_state.clone();
        let view_split_button_clone_set = view_split_button.clone();

        set_mode_action.connect_activate(move |_action, parameter: Option<&Variant>| {
            let Some(param) = parameter else {
                error!("view.set-mode action called without parameter");
                return;
            };

            let Some(mode_value) = param.get::<i32>() else {
                error!("view.set-mode action parameter is not an i32");
                return;
            };

            let new_mode = match mode_value {
                0 => Grid,
                1 => List,
                _ => {
                    warn!("Invalid view mode value: {}", mode_value);
                    return;
                }
            };

            // Check if state actually changed
            let current_state = state_clone_set.get_library_state();
            if current_state.view_mode == new_mode {
                debug!("View mode unchanged, skipping update");
                return;
            }

            info!("View mode changed to: {:?}", new_mode);

            // Update icon
            let icon_name = Self::get_view_icon_name(&new_mode);
            view_split_button_clone_set.set_icon_name(icon_name);

            // Update app state
            state_clone_set.update_view_options(current_state.current_tab, new_mode);
        });

        // Add action to the widget itself
        let action_group = SimpleActionGroup::new();
        action_group.add_action(&set_mode_action);
        view_split_button.insert_action_group("win", Some(&action_group));
    }

    /// Connects zoom button handlers for zoom in/out functionality.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `zoom_out_button` - Zoom out button widget
    /// * `zoom_in_button` - Zoom in button widget
    fn connect_zoom_button_handlers(
        app_state: &Arc<AppState>,
        zoom_out_button: &Button,
        zoom_in_button: &Button,
    ) {
        let state_clone_zoom_out = app_state.clone();
        let state_clone_zoom_in = app_state.clone();

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

    /// Creates and connects the settings button for preferences dialog.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `application` - Optional application reference for dialog parent
    /// * `library_db` - Optional library database reference for preferences dialog
    ///
    /// # Returns
    ///
    /// Configured settings button widget.
    fn create_settings_button(
        app_state: &Arc<AppState>,
        application: Option<&Arc<Application>>,
        library_db: Option<&Arc<LibraryDatabase>>,
    ) -> Button {
        let settings_button = Button::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Settings")
            .use_underline(true)
            .build();

        // Connect settings button to show preferences dialog
        let app_state_clone = app_state.clone();
        let application_clone = application.cloned();
        let library_db_clone = library_db.cloned();

        settings_button.connect_clicked(move |_| {
            if let Some(app) = &application_clone
                && let Some(db) = &library_db_clone
            {
                let preferences_dialog = PreferencesDialog::new(&app_state_clone, db.clone());

                // Get the active window as parent
                if let Some(window) = app.active_window() {
                    if let Some(app_window) = window.downcast_ref::<ApplicationWindow>() {
                        preferences_dialog.show(app_window);
                    } else {
                        // Fallback: show without parent
                        preferences_dialog.show_without_parent();
                    }
                } else {
                    // Fallback: show without parent
                    preferences_dialog.show_without_parent();
                }
            }
        });

        settings_button
    }

    /// Creates tab navigation buttons for Albums/Artists.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// Tuple of (`album_tab`, `artist_tab`, `tab_box`).
    fn create_tab_buttons(app_state: &Arc<AppState>) -> (ToggleButton, ToggleButton, Box) {
        let current_tab = app_state.get_library_state().current_tab;

        // Create Albums tab with both icon and text
        let album_icon = Image::builder().icon_name("folder-music-symbolic").build();
        let album_label = Label::builder().label("Albums").build();
        let album_box = Box::builder().orientation(Horizontal).spacing(6).build();
        album_box.append(&album_icon);
        album_box.append(&album_label);

        let album_tab = ToggleButton::builder()
            .child(&album_box)
            .tooltip_text("Browse albums")
            .use_underline(true)
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
            .use_underline(true)
            .active(current_tab == Artists)
            .has_frame(false)
            .build();

        // Set up mutual exclusivity for tab buttons
        artist_tab.set_group(Some(&album_tab));

        // Connect tab buttons to app state
        let state_clone_album = app_state.clone();
        let state_clone_artist = app_state.clone();
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

                debug!("Switching to Albums tab");

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

                debug!("Switching to Artists tab");

                // Update app state using lightweight navigation update
                state_clone_artist.update_view_options(Artists, current_state.view_mode);

                // Ensure album tab is not active
                album_tab_clone.set_active(false);
            }
        });

        // Create tab container box
        let tab_box = Box::builder().orientation(Horizontal).spacing(6).build();
        tab_box.append(&album_tab);
        tab_box.append(&artist_tab);

        (album_tab, artist_tab, tab_box)
    }

    /// Subscribes to view option changes and updates the view button icon.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `view_split_button` - View split button to update
    ///
    /// # Returns
    ///
    /// Join handle for the subscription.
    fn subscribe_to_view_options(
        app_state: &Arc<AppState>,
        view_split_button: &SplitButton,
    ) -> JoinHandle<()> {
        let state_clone = app_state.clone();
        let view_split_button_clone = view_split_button.clone();
        MainContext::default().spawn_local(async move {
            let rx = state_clone.subscribe();
            while let Ok(event) = rx.recv().await {
                if let ViewOptionsChanged { view_mode, .. } = event {
                    // Update icon based on new view mode
                    let icon_name = Self::get_view_icon_name(&view_mode);
                    view_split_button_clone.set_icon_name(icon_name);
                }
            }
        })
    }

    /// Returns the icon name for a given view mode.
    ///
    /// # Arguments
    ///
    /// * `view_mode` - View mode to get icon for
    ///
    /// # Returns
    ///
    /// Icon name string for the view mode.
    fn get_view_icon_name(view_mode: &ViewMode) -> &'static str {
        match view_mode {
            List => "view-list-symbolic",
            Grid => "view-grid-symbolic",
        }
    }
}

impl HeaderBar {
    /// Creates a header bar with default configuration.
    ///
    /// This is a convenience constructor that wraps [`HeaderBar::new`] with
    /// the application parameter set to `Some(application)`.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for library state access
    /// * `application` - Application instance for preferences dialog parent
    /// * `settings_manager` - Settings manager reference for configuration
    /// * `library_db` - Library database reference for preferences dialog
    ///
    /// # Returns
    ///
    /// A new `HeaderBar` instance with the application reference set.
    pub fn default_with_state(
        app_state: &Arc<AppState>,
        application: Application,
        settings_manager: Arc<SettingsManager>,
        library_db: Arc<LibraryDatabase>,
    ) -> Self {
        Self::new(
            app_state,
            Some(application),
            settings_manager,
            Some(library_db),
        )
    }

    /// Clears the search entry without triggering search debounce.
    ///
    /// This method sets a flag to prevent the search debounce from firing
    /// while the text is cleared programmatically.
    pub fn clear_search(&self) {
        *self.clearing_search.lock() = true;
        self.search_entry.set_text("");
        *self.clearing_search.lock() = false;
    }

    /// Closes the search entry by deactivating the search button.
    ///
    /// This method hides the search entry and clears the search text.
    pub fn close_search(&self) {
        self.search_button.set_active(false);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use {
        anyhow::{Result, bail},
        libadwaita::{Application, prelude::ButtonExt},
        parking_lot::RwLock,
        tokio::test,
    };

    use crate::{
        config::settings::SettingsManager,
        library::{database::LibraryDatabase, scanner::LibraryScanner},
        state::app_state::AppState,
        ui::header_bar::HeaderBar,
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_header_bar_creation() -> Result<()> {
        let app_state = AppState::new(
            Weak::new(),
            None::<Arc<RwLock<LibraryScanner>>>,
            Arc::new(RwLock::new(SettingsManager::new()?)),
        );
        let application = Some(
            Application::builder()
                .application_id("com.example.oxhidifi")
                .build(),
        );
        let settings_manager = Arc::new(SettingsManager::new()?);
        let library_db = LibraryDatabase::new().await?;
        let header_bar = HeaderBar::new(
            &Arc::new(app_state),
            application,
            settings_manager,
            Some(Arc::new(library_db)),
        );

        // Check icon names without requiring widget realization
        if header_bar.search_button.icon_name().as_deref() != Some("system-search-symbolic") {
            bail!("Search button icon should be 'system-search-symbolic'");
        }
        if header_bar.view_split_button.icon_name().as_deref() != Some("view-grid-symbolic") {
            bail!("View split button icon should be 'view-grid-symbolic'");
        }
        if header_bar.settings_button.icon_name().as_deref() != Some("open-menu-symbolic") {
            bail!("Settings button icon should be 'open-menu-symbolic'");
        }
        Ok(())
    }
}

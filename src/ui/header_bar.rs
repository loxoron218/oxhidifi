//! Adaptive header bar with search, navigation, and action controls.
//!
//! This module implements the header bar component that provides
//! essential controls for navigation, search, and application settings.
//! It supports adaptive layouts for different screen sizes.

use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    time::Duration,
};

use {
    libadwaita::{
        Application, ApplicationWindow, HeaderBar as LibadwaitaHeaderBar, SplitButton,
        gio::{Icon, Menu, MenuItem, SimpleAction, SimpleActionGroup},
        glib::{
            ControlFlow::Continue, JoinHandle, MainContext, SourceId, Variant, VariantTy,
            timeout_add_local, timeout_add_local_once,
        },
        gtk::{
            Align::Start,
            Box, Button, Image, Label, MenuButton,
            Orientation::{Horizontal, Vertical},
            Popover, SearchBar, SearchEntry, Separator, ToggleButton,
        },
        prelude::{
            ActionMapExt, BoxExt, ButtonExt, Cast, EditableExt, GtkApplicationExt, PopoverExt,
            ToggleButtonExt, WidgetExt,
        },
    },
    parking_lot::Mutex,
    tracing::{debug, error, info, warn},
};

use crate::{
    config::settings::SettingsManager,
    library::database::LibraryDatabase,
    state::{
        app_state::{
            AppState,
            AppStateEvent::ViewOptionsChanged,
            LibraryTab::{Albums, Artists},
            NavigationState::Library,
            ViewMode::{self, Grid, List},
        },
        zoom_manager::ZoomEvent::{GridZoomChanged, ListZoomChanged},
    },
    ui::preferences::dialog::PreferencesDialog,
};

/// Search display mode for the header bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchDisplayMode {
    /// Inline search entry (desktop/tablet).
    Inline,
    /// Search bar below header (mobile/smallest screens).
    Bar,
}

/// Type alias for search debounce timer handle.
type SearchDebounceHandle = Arc<Mutex<Option<SourceId>>>;

/// Type alias for search clearing flag.
type SearchClearingFlag = Arc<AtomicBool>;

/// Type alias for `setup_search_functionality` return tuple.
type SearchSetupResult = (
    ToggleButton,
    Box,
    SearchEntry,
    SearchEntry,
    SearchBar,
    SearchDebounceHandle,
    SearchDebounceHandle,
    SearchClearingFlag,
);

/// Search state containing debounce handles and clearing flag.
struct SearchState {
    /// Debounce timer handle for inline search entry.
    debounce_handle_entry: Arc<Mutex<Option<SourceId>>>,
    /// Debounce timer handle for mobile search bar entry.
    debounce_handle_bar: Arc<Mutex<Option<SourceId>>>,
    /// Flag to prevent search debounce during programmatic text clearing.
    clearing_search: Arc<AtomicBool>,
    /// Current search display mode.
    search_display_mode: Arc<Mutex<SearchDisplayMode>>,
}

/// Grouped state for search button toggle callback.
/// Groups related Rc references to reduce cloning overhead.
struct SearchToggleState {
    /// Search entry container box.
    search_entry_container: Rc<Box>,
    /// Search entry widget.
    search_entry: Rc<SearchEntry>,
    /// Search bar widget.
    search_bar: Rc<SearchBar>,
    /// Debounce handle for inline search entry.
    debounce_handle_entry: Arc<Mutex<Option<SourceId>>>,
    /// Debounce handle for mobile search bar.
    debounce_handle_bar: Arc<Mutex<Option<SourceId>>>,
    /// Flag to prevent debounce during programmatic clearing.
    clearing_search: Arc<AtomicBool>,
    /// Current search display mode.
    search_display_mode: Arc<Mutex<SearchDisplayMode>>,
    /// Application state reference.
    app_state: Arc<AppState>,
}

/// Grouped state for search stop (ESC) callback.
struct SearchStopState {
    /// Search toggle button.
    search_button: Rc<ToggleButton>,
    /// Search entry widget.
    search_entry: Rc<SearchEntry>,
    /// Search bar widget.
    search_bar: Rc<SearchBar>,
    /// Debounce handle for inline search entry.
    debounce_handle_entry: Arc<Mutex<Option<SourceId>>>,
    /// Debounce handle for mobile search bar.
    debounce_handle_bar: Arc<Mutex<Option<SourceId>>>,
    /// Flag to prevent debounce during programmatic clearing.
    clearing_search: Arc<AtomicBool>,
    /// Application state reference.
    app_state: Arc<AppState>,
}

/// Grouped state for mobile search bar ESC callback.
struct MobileSearchStopState {
    /// Search toggle button.
    search_button: Rc<ToggleButton>,
    /// Search entry widget.
    search_entry: Rc<SearchEntry>,
    /// Search bar widget.
    search_bar: Rc<SearchBar>,
    /// Debounce handle for mobile search bar.
    debounce_handle: Arc<Mutex<Option<SourceId>>>,
    /// Flag to prevent debounce during programmatic clearing.
    clearing_search: Arc<AtomicBool>,
    /// Application state reference.
    app_state: Arc<AppState>,
}

/// Adaptive header bar with search, navigation, and action controls.
///
/// The `HeaderBar` provides a consistent interface for application
/// navigation, search functionality, settings access, and album/artist tab navigation.
/// It adapts to different screen sizes using breakpoints.
pub struct HeaderBar {
    /// The underlying Libadwaita header bar widget.
    pub widget: LibadwaitaHeaderBar,
    /// Search toggle button.
    pub search_button: ToggleButton,
    /// View split button.
    pub view_split_button: SplitButton,
    /// Settings button (hidden on smallest screens).
    pub settings_button: Button,
    /// Merged menu button for smallest screens (view toggle + settings in popover).
    pub merged_menu_button: MenuButton,
    /// Application reference for preferences dialog.
    pub application: Option<Arc<Application>>,
    /// Search entry container for inline search (desktop/tablet).
    pub search_entry_container: Box,
    /// Search entry for inline search (desktop/tablet).
    pub search_entry: SearchEntry,
    /// Search bar for mobile/smallest screens.
    pub search_bar: SearchBar,
    /// Search entry for mobile search bar (smallest screens).
    pub search_entry_for_bar: SearchEntry,
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
    /// Debounce timer handle for inline search entry.
    search_debounce_handle_entry: Arc<Mutex<Option<SourceId>>>,
    /// Debounce timer handle for mobile search bar entry.
    search_debounce_handle_bar: Arc<Mutex<Option<SourceId>>>,
    /// Timer handle for zoom button sensitivity updates.
    zoom_timer_handle: Arc<Mutex<Option<SourceId>>>,
    /// Flag to prevent search debounce during programmatic text clearing.
    pub clearing_search: Arc<AtomicBool>,
    /// Current search display mode.
    search_display_mode: Arc<Mutex<SearchDisplayMode>>,
    /// Bulk action button for selection operations.
    pub bulk_action_button: Button,
    /// Popover for bulk selection actions.
    pub bulk_action_popover: Popover,
    /// Selection toggle in bulk action popover.
    pub selection_toggle: ToggleButton,
    /// Selection icon in bulk action popover.
    pub selection_icon: Image,
    /// Selection label in bulk action popover.
    pub selection_label: Label,
    /// Counter label showing number of selected items.
    pub selection_counter: Label,
    /// Bulk action controls box in merged menu (visible in adaptive mode).
    pub merged_menu_bulk_action_box: Box,
    /// Selection toggle in merged menu bulk action.
    pub merged_menu_selection_toggle: ToggleButton,
    /// Selection counter in merged menu bulk action.
    pub merged_menu_selection_counter: Label,
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

        let (back_button, search_display_mode) =
            Self::init_widget_and_back_button(&widget, app_state);

        let current_view_mode = app_state.get_library_state().view_mode;

        let (
            search_button,
            search_entry_container,
            search_entry,
            search_entry_for_bar,
            search_bar,
            debounce_handle_entry,
            debounce_handle_bar,
            clearing_search,
        ) = Self::setup_search_functionality(app_state, &settings_manager, &search_display_mode);

        widget.pack_start(&search_button);
        widget.pack_start(&search_entry_container);

        let (
            bulk_action_button,
            bulk_action_popover,
            selection_toggle,
            selection_icon,
            selection_label,
            selection_counter,
        ) = Self::init_bulk_action(app_state, &widget);

        let (view_split_button, zoom_popover, zoom_out_button, zoom_in_button, zoom_timer_handle) =
            Self::init_view_controls(app_state, &current_view_mode);

        let application_arc = application.map(Arc::new);
        let settings_button =
            Self::create_settings_button(app_state, application_arc.as_ref(), library_db.as_ref());
        widget.pack_end(&settings_button);
        widget.pack_end(&view_split_button);

        let (
            merged_menu_button,
            merged_menu_bulk_action_box,
            merged_menu_selection_toggle,
            merged_menu_selection_counter,
        ) = Self::create_merged_menu_button(
            app_state,
            application_arc.as_ref(),
            library_db.as_ref(),
            &view_split_button,
            &zoom_timer_handle,
        );
        widget.pack_end(&merged_menu_button);
        merged_menu_button.set_visible(false);

        let (album_tab, artist_tab, tab_box) = Self::create_tab_buttons(app_state);
        widget.set_title_widget(Some(&tab_box));

        let subscription_handle = Self::subscribe_to_view_options(app_state, &view_split_button);

        Self {
            widget,
            search_button,
            view_split_button,
            settings_button,
            merged_menu_button,
            search_entry_container,
            search_entry,
            search_entry_for_bar,
            search_bar,
            album_tab,
            artist_tab,
            tab_box,
            back_button,
            zoom_out_button,
            zoom_in_button,
            zoom_popover,
            app_state: Arc::clone(app_state),
            settings_manager,
            application: application_arc,
            current_view_mode,
            library_db,
            search_debounce_handle_entry: debounce_handle_entry,
            search_debounce_handle_bar: debounce_handle_bar,
            zoom_timer_handle,
            clearing_search,
            _subscription_handle: subscription_handle,
            search_display_mode,
            bulk_action_button,
            bulk_action_popover,
            selection_toggle,
            selection_icon,
            selection_label,
            selection_counter,
            merged_menu_bulk_action_box,
            merged_menu_selection_toggle,
            merged_menu_selection_counter,
        }
    }

    /// Initializes the header bar widget and back button.
    ///
    /// # Arguments
    ///
    /// * `widget` - The header bar widget to pack the back button into
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// Tuple of (`back_button`, `search_display_mode`).
    fn init_widget_and_back_button(
        widget: &LibadwaitaHeaderBar,
        app_state: &Arc<AppState>,
    ) -> (Button, Arc<Mutex<SearchDisplayMode>>) {
        let back_button = Self::create_back_button(app_state);
        widget.pack_start(&back_button);
        let search_display_mode = Arc::new(Mutex::new(SearchDisplayMode::Inline));
        (back_button, search_display_mode)
    }

    /// Initializes bulk action components (popover, button, and selection controls).
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `widget` - The header bar widget to pack the button into
    ///
    /// # Returns
    ///
    /// Tuple of (`bulk_action_button`, `bulk_action_popover`, `selection_toggle`,
    /// `selection_icon`, `selection_label`, `selection_counter`).
    fn init_bulk_action(
        app_state: &Arc<AppState>,
        widget: &LibadwaitaHeaderBar,
    ) -> (Button, Popover, ToggleButton, Image, Label, Label) {
        let (
            bulk_action_popover,
            selection_toggle,
            selection_icon,
            selection_label,
            selection_counter,
        ) = Self::create_bulk_action_popover(app_state);

        let bulk_action_button = Self::create_bulk_action_button(&bulk_action_popover);

        bulk_action_popover.set_parent(&bulk_action_button);
        widget.pack_start(&bulk_action_button);

        (
            bulk_action_button,
            bulk_action_popover,
            selection_toggle,
            selection_icon,
            selection_label,
            selection_counter,
        )
    }

    /// Initializes view controls (split button with menu, zoom popover, and zoom buttons).
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `current_view_mode` - Current view mode for icon initialization
    ///
    /// # Returns
    ///
    /// Tuple of (`view_split_button`, `zoom_popover`, `zoom_out_button`,
    /// `zoom_in_button`, `zoom_timer_handle`).
    fn init_view_controls(
        app_state: &Arc<AppState>,
        current_view_mode: &ViewMode,
    ) -> (
        SplitButton,
        Popover,
        Button,
        Button,
        Arc<Mutex<Option<SourceId>>>,
    ) {
        let menu = Self::create_view_menu();

        let view_split_button = SplitButton::builder()
            .icon_name(Self::get_view_icon_name(current_view_mode))
            .tooltip_text("Toggle View")
            .menu_model(&menu)
            .build();

        let (zoom_popover, zoom_out_button, zoom_in_button) = Self::create_zoom_popover();
        view_split_button.set_popover(Some(&zoom_popover));

        let zoom_timer_handle = Arc::new(Mutex::new(None));

        Self::connect_view_button_handlers(app_state, &view_split_button);

        Self::connect_zoom_button_handlers(app_state, &zoom_out_button, &zoom_in_button);

        Self::setup_zoom_buttons(
            app_state,
            &zoom_out_button,
            &zoom_in_button,
            &zoom_timer_handle,
        );

        (
            view_split_button,
            zoom_popover,
            zoom_out_button,
            zoom_in_button,
            zoom_timer_handle,
        )
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
        let state_clone = Arc::clone(app_state);
        back_button.connect_clicked(move |_| {
            // Navigate back to library root
            state_clone.update_navigation(Library);
        });

        back_button
    }

    /// Creates and configures the selection toggle button.
    ///
    /// # Arguments
    ///
    /// * `popover` - The popover to display when clicked
    ///
    /// # Returns
    ///
    /// Configured bulk action button widget.
    fn create_bulk_action_button(popover: &Popover) -> Button {
        let bulk_action_button = Button::builder()
            .icon_name("applications-utilities-symbolic")
            .tooltip_text("Bulk Actions")
            .visible(false)
            .build();

        let popover_clone = popover.clone();
        bulk_action_button.connect_clicked(move |_button| {
            popover_clone.popup();
        });

        bulk_action_button
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
    /// Tuple of (`search_button`, `search_entry_container`, `search_entry`,
    /// `search_entry_for_bar`, `search_bar`, `debounce_handle_entry`, `debounce_handle_bar`,
    /// `clearing_search_flag`).
    fn setup_search_functionality(
        app_state: &Arc<AppState>,
        settings_manager: &Arc<SettingsManager>,
        search_display_mode: &Arc<Mutex<SearchDisplayMode>>,
    ) -> SearchSetupResult {
        let (search_entry, search_entry_container, search_entry_for_bar, search_bar) =
            Self::create_search_widgets();

        let search_button = Self::create_search_button();

        let search_state = SearchState {
            debounce_handle_entry: Arc::new(Mutex::new(None)),
            debounce_handle_bar: Arc::new(Mutex::new(None)),
            clearing_search: Arc::new(AtomicBool::new(false)),
            search_display_mode: Arc::clone(search_display_mode),
        };

        Self::connect_search_button_toggle(
            &search_button,
            app_state,
            &search_entry_container,
            &search_entry,
            &search_bar,
            &search_state,
        );

        Self::connect_search_entry_handlers(
            &search_entry,
            app_state,
            &search_state,
            settings_manager,
        );

        Self::connect_search_bar_handlers(
            &search_entry_for_bar,
            app_state,
            &search_button,
            &search_state,
            settings_manager,
            &search_bar,
        );

        (
            search_button,
            search_entry_container,
            search_entry,
            search_entry_for_bar,
            search_bar,
            search_state.debounce_handle_entry,
            search_state.debounce_handle_bar,
            search_state.clearing_search,
        )
    }

    /// Creates search entry widgets for both inline and mobile search modes.
    ///
    /// This function creates:
    /// - An inline search entry for desktop/tablet layouts
    /// - A container box for the inline search entry
    /// - A separate search entry for the mobile search bar
    /// - A search bar widget that wraps the mobile search entry
    ///
    /// # Returns
    ///
    /// Tuple of (`search_entry`, `search_entry_container`, `search_entry_for_bar`, `search_bar`).
    fn create_search_widgets() -> (SearchEntry, Box, SearchEntry, SearchBar) {
        let search_entry = SearchEntry::builder()
            .placeholder_text("Search albums and artists...")
            .hexpand(true)
            .margin_start(12)
            .margin_end(12)
            .visible(true)
            .build();

        // Wrap in a box to ensure visibility
        let search_entry_container = Box::builder()
            .orientation(Horizontal)
            .visible(false)
            .hexpand(true)
            .build();
        search_entry_container.append(&search_entry);

        // Create a separate search entry for the search bar (mobile/small screens)
        let search_entry_for_bar = SearchEntry::builder()
            .placeholder_text("Search albums and artists...")
            .hexpand(true)
            .margin_start(12)
            .margin_end(12)
            .build();

        let search_bar = SearchBar::new();
        search_bar.set_search_mode(false);
        search_bar.set_visible(false);
        search_bar.set_child(Some(&search_entry_for_bar));

        (
            search_entry,
            search_entry_container,
            search_entry_for_bar,
            search_bar,
        )
    }

    /// Creates the search toggle button.
    ///
    /// # Returns
    ///
    /// A configured `ToggleButton` for search activation.
    fn create_search_button() -> ToggleButton {
        ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .use_underline(true)
            .build()
    }

    /// Connects handlers for the search button toggle behavior.
    ///
    /// # Arguments
    ///
    /// * `search_button` - The search toggle button
    /// * `app_state` - Application state for filter updates
    /// * `search_entry_container` - Container for inline search entry
    /// * `search_entry` - The inline search entry widget
    /// * `search_bar` - The mobile search bar widget
    /// * `search_state` - Search state containing debounce and clearing flags
    fn connect_search_button_toggle(
        search_button: &ToggleButton,
        app_state: &Arc<AppState>,
        search_entry_container: &Box,
        search_entry: &SearchEntry,
        search_bar: &SearchBar,
        search_state: &SearchState,
    ) {
        let toggle_state = SearchToggleState {
            search_entry_container: Rc::new(search_entry_container.clone()),
            search_entry: Rc::new(search_entry.clone()),
            search_bar: Rc::new(search_bar.clone()),
            debounce_handle_entry: Arc::clone(&search_state.debounce_handle_entry),
            debounce_handle_bar: Arc::clone(&search_state.debounce_handle_bar),
            clearing_search: Arc::clone(&search_state.clearing_search),
            search_display_mode: Arc::clone(&search_state.search_display_mode),
            app_state: Arc::clone(app_state),
        };

        search_button.connect_toggled(move |button: &ToggleButton| {
            let is_active = button.is_active();
            let display_mode = *toggle_state.search_display_mode.lock();

            if display_mode == SearchDisplayMode::Bar {
                // Small screen: toggle search bar below header
                toggle_state.search_bar.set_search_mode(is_active);

                // Focus on the search entry inside the search bar
                if is_active
                    && let Some(child) = toggle_state.search_bar.child()
                    && let Some(entry) = child.downcast_ref::<SearchEntry>()
                {
                    entry.grab_focus();
                }
            } else {
                // Large screen: toggle inline search entry
                toggle_state.search_entry_container.set_visible(is_active);

                if is_active {
                    toggle_state.search_entry.grab_focus();
                }
            }

            if !is_active {
                toggle_state.clearing_search.store(true, SeqCst);
                toggle_state.search_entry.set_text("");
                toggle_state.clearing_search.store(false, SeqCst);

                // Cancel any pending debounce timers for both search entries
                if let Some(timer_id) = toggle_state.debounce_handle_entry.lock().take() {
                    let () = timer_id.remove();
                }
                if let Some(timer_id) = toggle_state.debounce_handle_bar.lock().take() {
                    let () = timer_id.remove();
                }

                // Reset search filter when closing search
                toggle_state.app_state.update_search_filter(None);
            }
        });

        // Handle Escape to hide search entry
        let stop_state = SearchStopState {
            search_button: Rc::new(search_button.clone()),
            search_entry: Rc::new(search_entry.clone()),
            search_bar: Rc::new(search_bar.clone()),
            debounce_handle_entry: Arc::clone(&search_state.debounce_handle_entry),
            debounce_handle_bar: Arc::clone(&search_state.debounce_handle_bar),
            clearing_search: Arc::clone(&search_state.clearing_search),
            app_state: Arc::clone(app_state),
        };

        search_entry.connect_stop_search(move |_| {
            stop_state.search_button.set_active(false);
            stop_state.search_bar.set_search_mode(false);

            // Set flag first to block any in-flight timers before cancelling
            stop_state.clearing_search.store(true, SeqCst);

            // Cancel any pending debounce timers for both search entries
            if let Some(timer_id) = stop_state.debounce_handle_entry.lock().take() {
                let () = timer_id.remove();
            }
            if let Some(timer_id) = stop_state.debounce_handle_bar.lock().take() {
                let () = timer_id.remove();
            }

            // Clear search text and filter when ESC is pressed
            stop_state.search_entry.set_text("");
            stop_state.clearing_search.store(false, SeqCst);

            // Reset search filter
            stop_state.app_state.update_search_filter(None);
        });
    }

    /// Connects debounced search handler to a search entry.
    ///
    /// # Arguments
    ///
    /// * `search_entry` - The search entry widget
    /// * `app_state` - Application state for filter updates
    /// * `debounce_handle` - Shared debounce timer handle
    /// * `clearing_flag` - Flag to prevent debounce during programmatic clearing
    /// * `settings_manager` - Settings manager for debounce duration configuration
    fn connect_debounced_search(
        search_entry: &SearchEntry,
        app_state: &Arc<AppState>,
        debounce_handle: SearchDebounceHandle,
        clearing_flag: SearchClearingFlag,
        settings_manager: &Arc<SettingsManager>,
    ) {
        let state_clone = Arc::clone(app_state);
        let debounce_search = debounce_handle;
        let clearing_ch = clearing_flag;
        let settings_manager_search = Arc::clone(settings_manager);

        search_entry.connect_search_changed(move |entry| {
            let text = entry.text();

            // Cancel any pending debounce timer first
            if let Some(timer_id) = debounce_search.lock().take() {
                let () = timer_id.remove();
            }

            // Skip everything during programmatic text clearing
            if clearing_ch.load(SeqCst) {
                return;
            }

            let state = Arc::clone(&state_clone);

            // Update immediately if empty, otherwise debounce
            if text.is_empty() {
                state.update_search_filter(None);
            } else {
                let text = String::from(text);
                let handle_clone = Arc::clone(&debounce_search);
                let handle_clone_for_id = Arc::clone(&debounce_search);

                let debounce_ms = settings_manager_search.get_settings().search_debounce_ms;

                let timer_id =
                    timeout_add_local_once(Duration::from_millis(debounce_ms), move || {
                        state.update_search_filter(Some(text));
                        *handle_clone.lock() = None;
                    });

                *handle_clone_for_id.lock() = Some(timer_id);
            }
        });
    }

    /// Connects search handlers for the inline search entry.
    ///
    /// # Arguments
    ///
    /// * `search_entry` - The inline search entry widget
    /// * `app_state` - Application state for filter updates
    /// * `search_state` - Search state containing debounce and clearing flags
    /// * `settings_manager` - Settings manager for debounce duration configuration
    fn connect_search_entry_handlers(
        search_entry: &SearchEntry,
        app_state: &Arc<AppState>,
        search_state: &SearchState,
        settings_manager: &Arc<SettingsManager>,
    ) {
        Self::connect_debounced_search(
            search_entry,
            app_state,
            Arc::clone(&search_state.debounce_handle_entry),
            Arc::clone(&search_state.clearing_search),
            settings_manager,
        );
    }

    /// Connects search handlers for the mobile search bar.
    ///
    /// # Arguments
    ///
    /// * `search_entry_for_bar` - The search entry within the mobile search bar
    /// * `app_state` - Application state for filter updates
    /// * `search_button` - The search toggle button to sync state with
    /// * `search_state` - Search state containing debounce and clearing flags
    /// * `settings_manager` - Settings manager for debounce duration configuration
    /// * `search_bar` - The search bar widget to close on ESC
    fn connect_search_bar_handlers(
        search_entry_for_bar: &SearchEntry,
        app_state: &Arc<AppState>,
        search_button: &ToggleButton,
        search_state: &SearchState,
        settings_manager: &Arc<SettingsManager>,
        search_bar: &SearchBar,
    ) {
        Self::connect_debounced_search(
            search_entry_for_bar,
            app_state,
            Arc::clone(&search_state.debounce_handle_bar),
            Arc::clone(&search_state.clearing_search),
            settings_manager,
        );

        // Handle ESC on search bar entry
        let esc_state = MobileSearchStopState {
            search_button: Rc::new(search_button.clone()),
            search_entry: Rc::new(search_entry_for_bar.clone()),
            search_bar: Rc::new(search_bar.clone()),
            debounce_handle: Arc::clone(&search_state.debounce_handle_bar),
            clearing_search: Arc::clone(&search_state.clearing_search),
            app_state: Arc::clone(app_state),
        };

        search_entry_for_bar.connect_stop_search(move |_| {
            esc_state.search_button.set_active(false);
            esc_state.search_bar.set_search_mode(false);

            if let Some(timer_id) = esc_state.debounce_handle.lock().take() {
                let () = timer_id.remove();
            }

            esc_state.clearing_search.store(true, SeqCst);
            esc_state.search_entry.set_text("");
            esc_state.clearing_search.store(false, SeqCst);

            esc_state.app_state.update_search_filter(None);
        });
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
        let zoom_box = Box::builder()
            .orientation(Vertical)
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        // Create main horizontal container for label and zoom buttons
        let zoom_controls_box = Box::builder().orientation(Horizontal).spacing(6).build();

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

    /// Creates the bulk action popover with selection toggle.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for selection operations
    ///
    /// # Returns
    ///
    /// Tuple of (`bulk_action_popover`, `selection_toggle`, `selection_icon`, `selection_label`,
    /// `selection_counter`).
    fn create_bulk_action_popover(
        app_state: &Arc<AppState>,
    ) -> (Popover, ToggleButton, Image, Label, Label) {
        let selection_label = Label::builder().label("Select All").build();
        let selection_counter = Label::builder().label("0 selected").build();

        let popover_container = Box::builder()
            .orientation(Vertical)
            .spacing(6)
            .hexpand(true)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        popover_container.append(&selection_counter);

        let horizontal_box = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .hexpand(true)
            .build();

        let icon = Image::builder()
            .icon_name("edit-select-all-symbolic")
            .build();
        horizontal_box.append(&icon);
        horizontal_box.append(&selection_label);

        let selection_toggle = ToggleButton::builder()
            .child(&horizontal_box)
            .css_classes(["flat"])
            .hexpand(true)
            .build();

        popover_container.append(&selection_toggle);

        let popover = Popover::builder()
            .child(&popover_container)
            .has_arrow(true)
            .autohide(true)
            .build();

        let state_for_toggle = Arc::clone(app_state);
        let icon_for_toggle = icon.clone();
        selection_toggle.connect_clicked(move |_toggle| {
            let state = state_for_toggle.as_ref().get_library_state();

            let all_selected = match state.current_tab {
                Albums => {
                    !state.albums.is_empty() && state.selected_album_ids.len() == state.albums.len()
                }
                Artists => {
                    !state.artists.is_empty()
                        && state.selected_artist_ids.len() == state.artists.len()
                }
            };

            if all_selected {
                icon_for_toggle.set_icon_name(Some("edit-select-all-symbolic"));
                match state.current_tab {
                    Albums => state_for_toggle.clear_album_selection(),
                    Artists => state_for_toggle.clear_artist_selection(),
                }
            } else {
                icon_for_toggle.set_icon_name(Some("edit-delete-symbolic"));
                match state.current_tab {
                    Albums => state_for_toggle.select_all_albums(),
                    Artists => state_for_toggle.select_all_artists(),
                }
            }
        });

        (
            popover,
            selection_toggle,
            icon,
            selection_label,
            selection_counter,
        )
    }

    /// Creates and configures the bulk action button.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `view_split_button` - View split button widget
    fn connect_view_button_handlers(app_state: &Arc<AppState>, view_split_button: &SplitButton) {
        // Connect main button click to toggle view mode
        let state_clone_main = Arc::clone(app_state);
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
        let state_clone_set = Arc::clone(app_state);
        let view_split_button_clone_set = view_split_button.clone();

        set_mode_action.connect_activate(move |_action, parameter: Option<&Variant>| {
            let Some(param) = parameter else {
                error!(action = "view.set-mode", "Action called without parameter");
                return;
            };

            let Some(mode_value) = param.get::<i32>() else {
                error!(action = "view.set-mode", "Action parameter is not an i32");
                return;
            };

            let new_mode = match mode_value {
                0 => Grid,
                1 => List,
                _ => {
                    warn!(mode_value = mode_value, "Invalid view mode value");
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
        let state_clone_zoom_out = Arc::clone(app_state);
        let state_clone_zoom_in = Arc::clone(app_state);

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
        let app_state_clone = Arc::clone(app_state);
        let application_clone = application.cloned();
        let library_db_clone = library_db.cloned();

        settings_button.connect_clicked(move |_| {
            if let Some(app) = &application_clone
                && let Some(db) = &library_db_clone
            {
                let preferences_dialog = PreferencesDialog::new(&app_state_clone, Arc::clone(db));

                // Get the active window as parent
                if let Some(window) = app.active_window() {
                    if let Some(app_window) = window.downcast_ref::<ApplicationWindow>() {
                        preferences_dialog.show(app_window);
                    } else {
                        // Fallback: show without parent
                        warn!(
                            widget_type = "ApplicationWindow",
                            "Active window is not ApplicationWindow, showing without parent"
                        );
                        preferences_dialog.show_without_parent();
                    }
                } else {
                    // Fallback: show without parent
                    warn!(
                        window_type = "ApplicationWindow",
                        "No active window found, showing dialog without parent"
                    );
                    preferences_dialog.show_without_parent();
                }
            }
        });

        settings_button
    }

    /// Creates the merged menu button for smallest screens.
    ///
    /// This combines view toggle, zoom controls, and settings into a single popover.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `application` - Optional application reference for dialog parent
    /// * `library_db` - Optional library database reference for preferences dialog
    /// * `view_split_button` - Reference to sync view icon with
    /// * `zoom_timer_handle` - Timer handle for the periodic zoom update timer
    ///
    /// # Returns
    ///
    /// Tuple of (`MenuButton`, `bulk_action_box`, `selection_toggle`, `selection_counter`).
    fn create_merged_menu_button(
        app_state: &Arc<AppState>,
        application: Option<&Arc<Application>>,
        library_db: Option<&Arc<LibraryDatabase>>,
        view_split_button: &SplitButton,
        zoom_timer_handle: &Arc<Mutex<Option<SourceId>>>,
    ) -> (MenuButton, Box, ToggleButton, Label) {
        let menu = Menu::new();

        let view_item = MenuItem::new(Some("Toggle View"), None);
        if let Ok(icon) = Icon::for_string("view-grid-symbolic") {
            view_item.set_icon(&icon);
        }
        menu.append_item(&view_item);

        let menu_box = Box::builder()
            .orientation(Vertical)
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        let view_icon_for_action = view_split_button
            .icon_name()
            .as_deref()
            .unwrap_or("view-grid-symbolic")
            .to_string();

        let view_toggle_button = Self::create_view_toggle_button(app_state, view_icon_for_action);
        menu_box.append(&view_toggle_button);

        let bulk_action_separator = Separator::new(Horizontal);
        menu_box.append(&bulk_action_separator);

        let (bulk_action_box, merged_menu_selection_toggle, merged_menu_selection_counter) =
            Self::create_merged_menu_bulk_action(app_state);

        menu_box.append(&bulk_action_box);

        let separator = Separator::new(Horizontal);
        menu_box.append(&separator);

        let zoom_controls_box = Self::create_zoom_controls_box(app_state, zoom_timer_handle);
        menu_box.append(&zoom_controls_box);

        let settings_separator = Separator::new(Horizontal);
        menu_box.append(&settings_separator);

        let settings_button =
            Self::create_merged_settings_button(app_state, application, library_db);
        menu_box.append(&settings_button);

        let popover = Popover::builder().child(&menu_box).has_arrow(true).build();

        let zoom_timer_handle_closed = Arc::clone(zoom_timer_handle);
        popover.connect_closed(move |_| {
            if let Some(timer_id) = zoom_timer_handle_closed.lock().take() {
                let () = timer_id.remove();
            }
        });

        let menu_button = MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Menu")
            .popover(&popover)
            .use_underline(true)
            .build();

        (
            menu_button,
            bulk_action_box,
            merged_menu_selection_toggle,
            merged_menu_selection_counter,
        )
    }

    /// Creates the bulk action controls for the merged menu.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// Tuple of (`bulk_action_box`, `selection_toggle`, `selection_counter`).
    fn create_merged_menu_bulk_action(app_state: &Arc<AppState>) -> (Box, ToggleButton, Label) {
        let bulk_action_box = Box::builder()
            .orientation(Vertical)
            .spacing(6)
            .hexpand(true)
            .margin_top(6)
            .visible(false)
            .build();

        let merged_menu_selection_counter = Label::builder()
            .label("0 selected")
            .halign(Start)
            .hexpand(true)
            .build();
        bulk_action_box.append(&merged_menu_selection_counter);

        let horizontal_box = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .hexpand(true)
            .build();

        let icon = Image::builder()
            .icon_name("edit-select-all-symbolic")
            .build();
        horizontal_box.append(&icon);

        let selection_label = Label::builder().label("Select All").build();
        horizontal_box.append(&selection_label);

        let merged_menu_selection_toggle = ToggleButton::builder()
            .child(&horizontal_box)
            .css_classes(["flat"])
            .hexpand(true)
            .build();

        let state_for_toggle = Arc::clone(app_state);
        let icon_for_toggle = icon;
        merged_menu_selection_toggle.connect_clicked(move |_toggle| {
            let state = state_for_toggle.as_ref().get_library_state();

            let all_selected = match state.current_tab {
                Albums => {
                    !state.albums.is_empty() && state.selected_album_ids.len() == state.albums.len()
                }
                Artists => {
                    !state.artists.is_empty()
                        && state.selected_artist_ids.len() == state.artists.len()
                }
            };

            if all_selected {
                icon_for_toggle.set_icon_name(Some("edit-delete-symbolic"));
                match state.current_tab {
                    Albums => state_for_toggle.clear_album_selection(),
                    Artists => state_for_toggle.clear_artist_selection(),
                }
            } else {
                icon_for_toggle.set_icon_name(Some("edit-select-all-symbolic"));
                match state.current_tab {
                    Albums => state_for_toggle.select_all_albums(),
                    Artists => state_for_toggle.select_all_artists(),
                }
            }
        });

        bulk_action_box.append(&merged_menu_selection_toggle);

        (
            bulk_action_box,
            merged_menu_selection_toggle,
            merged_menu_selection_counter,
        )
    }

    /// Creates a button to toggle between grid and list view modes.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `icon_name` - Initial icon name for the view mode
    ///
    /// # Returns
    ///
    /// A `Button` that toggles the view mode when clicked.
    fn create_view_toggle_button(app_state: &Arc<AppState>, icon_name: String) -> Button {
        let view_toggle_box = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .hexpand(true)
            .build();

        let view_icon = Image::builder().icon_name(icon_name).build();
        let view_label = Label::builder().label("Toggle View").build();
        view_toggle_box.append(&view_icon);
        view_toggle_box.append(&view_label);

        let view_toggle_button = Button::builder()
            .child(&view_toggle_box)
            .css_classes(["flat"])
            .hexpand(true)
            .build();

        let state_for_view_btn = Arc::clone(app_state);
        let view_icon_clone = view_icon;
        view_toggle_button.connect_clicked(move |_| {
            let current_state = state_for_view_btn.get_library_state();
            let new_mode = if current_state.view_mode == Grid {
                List
            } else {
                Grid
            };

            if current_state.view_mode != new_mode {
                let icon_name = match new_mode {
                    List => "view-list-symbolic",
                    Grid => "view-grid-symbolic",
                };
                view_icon_clone.set_icon_name(Some(icon_name));
                state_for_view_btn.update_view_options(current_state.current_tab, new_mode);
            }
        });

        view_toggle_button
    }

    /// Creates zoom in/out controls for the merged menu button.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `zoom_timer_handle` - Timer handle to store the periodic timer ID
    ///
    /// # Returns
    ///
    /// A `Box` containing zoom out label, zoom in label, and icon size label.
    fn create_zoom_controls_box(
        app_state: &Arc<AppState>,
        zoom_timer_handle: &Arc<Mutex<Option<SourceId>>>,
    ) -> Box {
        let zoom_controls_box = Box::builder()
            .orientation(Vertical)
            .spacing(6)
            .hexpand(true)
            .build();

        let zoom_out_btn =
            Self::create_zoom_button(app_state, "list-remove-symbolic", "Zoom Out", true);
        let zoom_in_btn =
            Self::create_zoom_button(app_state, "list-add-symbolic", "Zoom In", false);

        zoom_controls_box.append(&zoom_out_btn);
        zoom_controls_box.append(&zoom_in_btn);

        Self::setup_zoom_buttons(app_state, &zoom_out_btn, &zoom_in_btn, zoom_timer_handle);

        zoom_controls_box
    }

    /// Creates a zoom button with icon and label.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `icon_name` - Icon name for the button
    /// * `tooltip_text` - Tooltip text for the button
    /// * `is_zoom_out` - Whether this is a zoom out button
    ///
    /// # Returns
    ///
    /// Configured zoom button widget.
    fn create_zoom_button(
        app_state: &Arc<AppState>,
        icon_name: &str,
        tooltip_text: &str,
        is_zoom_out: bool,
    ) -> Button {
        let zoom_box = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .hexpand(true)
            .build();

        let zoom_icon = Image::builder().icon_name(icon_name).build();
        let zoom_label = Label::builder()
            .label(if is_zoom_out { "Zoom Out" } else { "Zoom In" })
            .halign(Start)
            .hexpand(true)
            .build();

        zoom_box.append(&zoom_icon);
        zoom_box.append(&zoom_label);

        let state_clone = Arc::clone(app_state);
        let zoom_btn = Button::builder()
            .child(&zoom_box)
            .tooltip_text(tooltip_text)
            .css_classes(["flat"])
            .hexpand(true)
            .build();

        if is_zoom_out {
            zoom_btn.connect_clicked(move |_| {
                let current_view_mode = state_clone.get_library_state().view_mode;
                match current_view_mode {
                    Grid => {
                        state_clone.decrease_grid_zoom_level();
                    }
                    List => {
                        state_clone.decrease_list_zoom_level();
                    }
                }
            });
        } else {
            zoom_btn.connect_clicked(move |_| {
                let current_view_mode = state_clone.get_library_state().view_mode;
                match current_view_mode {
                    Grid => {
                        state_clone.increase_grid_zoom_level();
                    }
                    List => {
                        state_clone.increase_list_zoom_level();
                    }
                }
            });
        }

        zoom_btn
    }

    /// Sets up zoom button sensitivity and periodic updates.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `zoom_out_btn` - Zoom out button widget
    /// * `zoom_in_btn` - Zoom in button widget
    /// * `zoom_timer_handle` - Timer handle to store the periodic timer ID
    fn setup_zoom_buttons(
        app_state: &Arc<AppState>,
        zoom_out_btn: &Button,
        zoom_in_btn: &Button,
        zoom_timer_handle: &Arc<Mutex<Option<SourceId>>>,
    ) {
        let (min_zoom, max_zoom, current) = Self::get_zoom_bounds(app_state);

        zoom_out_btn.set_sensitive(current > min_zoom);
        zoom_in_btn.set_sensitive(current < max_zoom);

        let app_state_clone = Arc::clone(app_state);
        let zoom_out_btn_clone = zoom_out_btn.clone();
        let zoom_in_btn_clone = zoom_in_btn.clone();

        let zoom_receiver = app_state_clone.zoom_manager.subscribe();
        let state_receiver = app_state_clone.subscribe();

        let timer_id = timeout_add_local(Duration::from_millis(100), move || {
            while let Ok(event) = state_receiver.try_recv() {
                if let ViewOptionsChanged { .. } = &*event {
                    let (min_zoom, max_zoom, current) = Self::get_zoom_bounds(&app_state_clone);
                    zoom_out_btn_clone.set_sensitive(current > min_zoom);
                    zoom_in_btn_clone.set_sensitive(current < max_zoom);
                }
            }

            while let Ok(event) = zoom_receiver.try_recv() {
                match event.as_ref() {
                    GridZoomChanged(_) | ListZoomChanged(_) => {
                        let (min_zoom, max_zoom, current) = Self::get_zoom_bounds(&app_state_clone);
                        zoom_out_btn_clone.set_sensitive(current > min_zoom);
                        zoom_in_btn_clone.set_sensitive(current < max_zoom);
                    }
                }
            }

            Continue
        });

        *zoom_timer_handle.lock() = Some(timer_id);
    }

    /// Gets zoom bounds and current level for the active view mode.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// Tuple of (`min_zoom`, `max_zoom`, `current_zoom_level`).
    fn get_zoom_bounds(app_state: &Arc<AppState>) -> (u8, u8, u8) {
        let view_mode = app_state.get_library_state().view_mode;
        let (grid_level, list_level) = {
            let zm = app_state.zoom_manager.as_ref();
            (zm.get_grid_zoom_level(), zm.get_list_zoom_level())
        };

        match view_mode {
            Grid => (0u8, 4u8, grid_level),
            List => (0u8, 2u8, list_level),
        }
    }

    /// Creates a settings button for the merged menu button.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `application` - Optional application reference for dialog parent
    /// * `library_db` - Optional library database reference for preferences dialog
    ///
    /// # Returns
    ///
    /// A `Button` that opens the preferences dialog when clicked.
    fn create_merged_settings_button(
        app_state: &Arc<AppState>,
        application: Option<&Arc<Application>>,
        library_db: Option<&Arc<LibraryDatabase>>,
    ) -> Button {
        let settings_row_box = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .hexpand(true)
            .build();

        let settings_icon = Image::builder()
            .icon_name("preferences-system-symbolic")
            .build();
        let settings_label = Label::builder().label("Settings").build();
        settings_row_box.append(&settings_icon);
        settings_row_box.append(&settings_label);

        let settings_button_merged = Button::builder()
            .child(&settings_row_box)
            .css_classes(["flat"])
            .hexpand(true)
            .build();

        let app_state_settings = Arc::clone(app_state);
        let application_settings = application.cloned();
        let library_db_settings = library_db.cloned();
        settings_button_merged.connect_clicked(move |_| {
            if let Some(app) = &application_settings
                && let Some(db) = &library_db_settings
            {
                let preferences_dialog =
                    PreferencesDialog::new(&app_state_settings, Arc::clone(db));

                if let Some(window) = app.active_window() {
                    if let Some(app_window) = window.downcast_ref::<ApplicationWindow>() {
                        preferences_dialog.show(app_window);
                    } else {
                        warn!(
                            widget_type = "ApplicationWindow",
                            "Active window is not ApplicationWindow, showing without parent"
                        );
                        preferences_dialog.show_without_parent();
                    }
                } else {
                    warn!(
                        window_type = "ApplicationWindow",
                        "No active window found, showing dialog without parent"
                    );
                    preferences_dialog.show_without_parent();
                }
            }
        });

        settings_button_merged
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
        let state_clone_album = Arc::clone(app_state);
        let state_clone_artist = Arc::clone(app_state);
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

                // Clear artist selection when switching tabs
                state_clone_album.clear_artist_selection();

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

                // Clear album selection when switching tabs
                state_clone_artist.clear_album_selection();

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
        let state_clone = Arc::clone(app_state);
        let view_split_button_clone = view_split_button.clone();
        MainContext::default().spawn_local(async move {
            let rx = state_clone.subscribe();
            while let Ok(event) = rx.recv().await {
                if let ViewOptionsChanged { view_mode, .. } = &*event {
                    // Update icon based on new view mode
                    let icon_name = Self::get_view_icon_name(view_mode);
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

impl Drop for HeaderBar {
    fn drop(&mut self) {
        if let Some(timer_id) = self.zoom_timer_handle.lock().take() {
            let () = timer_id.remove();
        }
        if let Some(timer_id) = self.search_debounce_handle_entry.lock().take() {
            let () = timer_id.remove();
        }
        if let Some(timer_id) = self.search_debounce_handle_bar.lock().take() {
            let () = timer_id.remove();
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
    #[must_use]
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
        self.clearing_search.store(true, SeqCst);
        self.search_entry.set_text("");
        self.search_entry_for_bar.set_text("");
        self.clearing_search.store(false, SeqCst);
    }

    /// Closes the search entry by deactivating the search button.
    ///
    /// This method hides the search entry and clears the search text.
    pub fn close_search(&self) {
        self.search_button.set_active(false);
    }

    /// Sets the header bar to adaptive/narrow mode for smallest screens.
    ///
    /// When enabled:
    /// - Settings and View buttons are hidden
    /// - Merged menu button is shown
    /// - Search bar mode is enabled (inline search entry hidden)
    ///
    /// Search text and active state are transferred between the inline
    /// entry and the mobile search bar so the user's search persists
    /// across breakpoint transitions.
    pub fn set_adaptive_mode(&self, adaptive: bool) {
        let search_was_active = self.search_button.is_active();

        // Capture text from the outgoing search entry before switching (only if active)
        let current_text = if search_was_active {
            if adaptive {
                Some(self.search_entry.text().to_string())
            } else {
                Some(self.search_entry_for_bar.text().to_string())
            }
        } else {
            None
        };

        let new_mode = if adaptive {
            SearchDisplayMode::Bar
        } else {
            SearchDisplayMode::Inline
        };
        *self.search_display_mode.lock() = new_mode;

        // Set flag to block any in-flight timers before cancelling
        self.clearing_search.store(true, SeqCst);
        if let Some(timer_id) = self.search_debounce_handle_entry.lock().take() {
            // SourceId::remove() returns (), no error handling needed
            // The timer is guaranteed to be valid when removed
            let () = timer_id.remove();
        }
        if let Some(timer_id) = self.search_debounce_handle_bar.lock().take() {
            // SourceId::remove() returns (), no error handling needed
            // The timer is guaranteed to be valid when removed
            let () = timer_id.remove();
        }

        if adaptive {
            self.settings_button.set_visible(false);
            self.view_split_button.set_visible(false);
            self.merged_menu_button.set_visible(true);
            self.bulk_action_button.set_visible(false);

            let is_on_library = matches!(self.app_state.get_navigation_state(), Library);
            self.merged_menu_bulk_action_box.set_visible(is_on_library);

            // Hide the inline entry, show the search bar
            self.search_entry_container.set_visible(false);
            self.search_bar.set_visible(true);

            // Transfer text and active state to the mobile search bar
            if search_was_active {
                self.clearing_search.store(true, SeqCst);
                self.search_entry_for_bar
                    .set_text(current_text.as_deref().unwrap_or(""));
                self.search_entry_for_bar.set_position(-1);

                self.search_bar.set_search_mode(true);
                self.clearing_search.store(false, SeqCst);
            }
        } else {
            self.settings_button.set_visible(true);
            self.view_split_button.set_visible(true);
            self.merged_menu_button.set_visible(false);
            self.bulk_action_button.set_visible(true);
            self.merged_menu_bulk_action_box.set_visible(false);

            // Wrap the UI state updates in clearing_search to prevent synchronous
            // search-changed signals from clearing the AppState filter.
            self.clearing_search.store(true, SeqCst);

            // Hide the search bar, conditionally show the inline entry
            self.search_bar.set_search_mode(false);
            self.search_bar.set_visible(false);

            // Transfer text and active state to the inline entry
            if search_was_active {
                self.search_entry
                    .set_text(current_text.as_deref().unwrap_or(""));
                self.search_entry.set_position(-1);

                self.search_entry_container.set_visible(true);

                let search_entry_clone = self.search_entry.clone();
                timeout_add_local_once(Duration::from_millis(50), move || {
                    search_entry_clone.grab_focus();
                    search_entry_clone.set_position(-1);
                });
            } else {
                self.search_entry_container.set_visible(false);
            }

            self.clearing_search.store(false, SeqCst);
        }
    }

    /// Returns whether the header bar is in adaptive mode.
    pub fn is_adaptive(&self) -> bool {
        *self.search_display_mode.lock() == SearchDisplayMode::Bar
    }

    /// Gets the search bar widget for placement below the header bar.
    ///
    /// This should be used in adaptive mode where the search bar
    /// appears below the header bar instead of inline.
    pub fn get_search_bar(&self) -> &SearchBar {
        &self.search_bar
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use {
        anyhow::{Result, bail},
        libadwaita::{Application, prelude::ButtonExt},
        parking_lot::RwLock,
        tokio::{sync::RwLock as TokioRwLock, test},
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
            None::<Arc<TokioRwLock<LibraryScanner>>>,
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

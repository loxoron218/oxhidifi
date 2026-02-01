//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{fs::read_to_string, path::Path, rc::Rc, sync::Arc};

use {
    libadwaita::{
        Application, ApplicationWindow, NavigationPage, NavigationView,
        gdk::{Display, Key},
        glib::{
            MainContext,
            Propagation::{Proceed, Stop},
        },
        gtk::{
            Box as GtkBox, CssProvider, EventControllerKey, Orientation::Vertical,
            STYLE_PROVIDER_PRIORITY_APPLICATION, ScrolledWindow, Stack,
            StackTransitionType::Crossfade, Widget, style_context_add_provider_for_display,
        },
        prelude::{
            AdjustmentExt, AdwApplicationWindowExt, ApplicationExt, ApplicationExtManual, BoxExt,
            Cast, GtkWindowExt, NavigationPageExt, WidgetExt,
        },
    },
    parking_lot::RwLock,
    tracing::{debug, error},
};

use crate::{
    audio::{
        engine::{
            AudioEngine,
            PlaybackState::{Buffering, Paused, Playing, Ready, Stopped},
        },
        queue_manager::QueueManager,
    },
    config::SettingsManager,
    error::domain::UiError::{self, InitializationError},
    library::{
        LibraryDatabase,
        scanner::{LibraryScanner, ScannerEvent::LibraryChanged},
    },
    state::{
        AppState,
        AppStateEvent::{
            LibraryDataChanged, NavigationChanged, PlaybackStateChanged, SearchFilterChanged,
            SettingsChanged, ViewOptionsChanged,
        },
        NavigationState::{AlbumDetail, ArtistDetail, Library},
        ViewMode::{Grid, List},
        app_state::LibraryTab::{Albums as LibraryAlbums, Artists as LibraryArtists},
    },
    ui::{
        header_bar::HeaderBar,
        player_bar::PlayerBar,
        views::{
            AlbumGridView, ArtistGridView, DetailView, ListView,
            detail_view::DetailType::{Album as AlbumDetailType, Artist as ArtistDetailType},
            list_view::ListViewType::{Albums, Artists},
        },
    },
};

/// Main application class with window management.
///
/// The `OxhidifiApplication` manages the main application window,
/// handles application lifecycle events, and coordinates between
/// different UI components.
pub struct OxhidifiApplication {
    /// The main application instance.
    pub app: Application,
    /// Audio engine for playback functionality.
    pub audio_engine: Arc<AudioEngine>,
    /// Library database for music library operations.
    pub library_db: Arc<LibraryDatabase>,
    /// Library scanner for real-time monitoring.
    pub library_scanner: Option<Arc<RwLock<LibraryScanner>>>,
    /// Application state manager.
    pub app_state: Arc<AppState>,
    /// Queue manager for playback queue operations.
    pub queue_manager: Arc<QueueManager>,
    /// User settings manager.
    pub settings: Arc<SettingsManager>,
}

impl OxhidifiApplication {
    /// Creates a new Oxhidifi application instance.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `OxhidifiApplication` or an error.
    ///
    /// # Errors
    ///
    /// Returns an error if audio engine, library database, or settings initialization fails.
    pub async fn new() -> Result<Self, UiError> {
        // Initialize settings
        let settings_manager = SettingsManager::new()
            .map_err(|e| InitializationError(format!("Failed to initialize settings: {e}")))?;
        let settings_manager_shared = Arc::new(settings_manager);

        // Extract UserSettings for components that need direct access
        let user_settings = settings_manager_shared.get_settings().clone();
        let user_settings_shared = Arc::new(RwLock::new(user_settings));

        // Initialize audio engine
        let audio_engine = AudioEngine::new()
            .map_err(|e| InitializationError(format!("Failed to initialize audio engine: {e}")))?;

        // Initialize queue manager
        let audio_engine_arc = Arc::new(audio_engine.clone());
        let track_finished_rx = audio_engine.subscribe_to_track_completion();

        // Initialize library database
        let library_db_raw = LibraryDatabase::new().await.map_err(|e| {
            InitializationError(format!("Failed to initialize library database: {e}"))
        })?;
        let library_db = Arc::new(library_db_raw);

        // Initialize library scanner if there are library directories
        let library_scanner = if user_settings_shared.read().library_directories.is_empty() {
            None
        } else {
            let scanner =
                LibraryScanner::new(&library_db, &user_settings_shared, None).map_err(|e| {
                    InitializationError(format!("Failed to initialize library scanner: {e}"))
                })?;

            // Perform initial scan of existing directories
            if let Err(e) = scanner
                .scan_initial_directories(&library_db, &user_settings_shared)
                .await
            {
                error!("Failed to perform initial library scan: {e}");
            }

            Some(Arc::new(RwLock::new(scanner)))
        };

        // Create application state
        let app_state = AppState::new(
            Arc::downgrade(&audio_engine_arc),
            library_scanner.clone(),
            Arc::new(RwLock::new((*settings_manager_shared).clone())),
        );

        // Initialize queue manager
        let queue_manager = QueueManager::new(
            audio_engine_arc.clone(),
            Arc::new(app_state.clone()),
            track_finished_rx,
        );
        let queue_manager = Arc::new(queue_manager);

        // Always load existing library data from database on startup
        // This ensures library is displayed even if no directories are currently configured
        if let Err(e) = library_db.cleanup_orphaned_records().await {
            error!("Failed to cleanup orphaned records: {e}");
        }

        let albums = match library_db.get_albums(None).await {
            Ok(albums) => albums,
            Err(e) => {
                error!("Failed to get albums from database: {e}");
                Vec::new()
            }
        };

        let artists = match library_db.get_artists(None).await {
            Ok(artists) => artists,
            Err(e) => {
                error!("Failed to get artists from database: {e}");
                Vec::new()
            }
        };

        // Update AppState with library data
        app_state.update_library_data(albums, artists);

        let app = Application::builder()
            .application_id("com.example.oxhidifi")
            .build();

        Ok(OxhidifiApplication {
            app,
            audio_engine: audio_engine_arc,
            library_db,
            library_scanner,
            app_state: Arc::new(app_state),
            queue_manager,
            settings: settings_manager_shared.clone(),
        })
    }

    /// Runs the application.
    ///
    /// This method starts the GTK main loop and displays the main window.
    pub fn run(&self) {
        self.app.connect_activate({
            let app_clone = self.app.clone();
            let audio_engine_clone = self.audio_engine.clone();
            let library_db_clone = self.library_db.clone();
            let app_state_clone = self.app_state.clone();
            let settings_manager_clone = self.settings.clone();
            let queue_manager_clone = self.queue_manager.clone();

            move |_| {
                build_ui(
                    &app_clone,
                    &audio_engine_clone,
                    &library_db_clone,
                    &app_state_clone,
                    &settings_manager_clone,
                    &queue_manager_clone,
                );

                // Subscribe to library scanner events if active
                if let Some(scanner_lock) = &app_state_clone.library_scanner.read().clone() {
                    let scanner = scanner_lock.read();
                    let rx = scanner.subscribe();
                    let app_state_refresh = app_state_clone.clone();
                    let db_refresh = library_db_clone.clone();

                    MainContext::default().spawn_local(async move {
                        loop {
                            match rx.recv().await {
                                Ok(LibraryChanged) => {
                                    debug!("LibraryChanged event received, refreshing app state");

                                    // Refresh albums
                                    let albums = match db_refresh.get_albums(None).await {
                                        Ok(albums) => albums,
                                        Err(e) => {
                                            error!("Failed to refresh albums: {e}");
                                            Vec::new()
                                        }
                                    };

                                    // Refresh artists
                                    let artists = match db_refresh.get_artists(None).await {
                                        Ok(artists) => artists,
                                        Err(e) => {
                                            error!("Failed to refresh artists: {e}");
                                            Vec::new()
                                        }
                                    };

                                    // Update state
                                    app_state_refresh.update_library_data(albums, artists);
                                }
                                Err(_) => {
                                    debug!("Scanner event channel closed");
                                    break;
                                }
                            }
                        }
                    });
                }
            }
        });

        self.app.run();
    }
}

/// Builds the main user interface.
fn build_ui(
    app: &Application,
    audio_engine: &Arc<AudioEngine>,
    library_db: &Arc<LibraryDatabase>,
    app_state: &Arc<AppState>,
    settings_manager: &Arc<SettingsManager>,
    queue_manager: &Arc<QueueManager>,
) {
    // Create the main window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(1200)
        .default_height(800)
        .build();

    // Create navigation view for handling view transitions
    let navigation_view = NavigationView::builder().build();

    // Create header bar with proper state integration
    let header_bar = Rc::new(HeaderBar::default_with_state(
        app_state,
        app.clone(),
        settings_manager.clone(),
    ));

    // Create main content area with responsive layout
    let main_content = create_main_content(
        app_state,
        settings_manager,
        library_db,
        audio_engine,
        queue_manager,
        &window,
        &header_bar,
    );

    // Add main content as root page
    let main_page = NavigationPage::builder()
        .child(&main_content)
        .title("Main")
        .build();
    navigation_view.add(&main_page);

    // Handle navigation events and update HeaderBar state centrally
    let navigation_view_clone = navigation_view.clone();
    let app_state_nav = app_state.clone();
    let hb_widget = header_bar.widget.clone();
    let hb_back = header_bar.back_button.clone();
    let hb_search = header_bar.search_button.clone();
    let hb_view = header_bar.view_split_button.clone();
    let hb_tabs = header_bar.tab_box.clone();

    MainContext::default().spawn_local(async move {
        let receiver = app_state_nav.subscribe();
        while let Ok(event) = receiver.recv().await {
            if let NavigationChanged(nav_state) = event {
                match *nav_state {
                    Library => {
                        let is_at_root = navigation_view_clone.visible_page().and_then(|p| p.tag())
                            == Some("root".into());

                        if !is_at_root {
                            navigation_view_clone.pop_to_tag("root");
                        }

                        hb_back.set_visible(false);
                        hb_search.set_visible(true);
                        hb_view.set_visible(true);
                        hb_widget.set_title_widget(Some(&hb_tabs));
                        hb_widget.set_show_start_title_buttons(true);
                        hb_widget.set_show_end_title_buttons(true);
                    }
                    AlbumDetail(album) => {
                        let detail_view = DetailView::builder()
                            .app_state(app_state_nav.clone())
                            .detail_type(AlbumDetailType(album.clone()))
                            .compact(false)
                            .build();

                        let page = NavigationPage::builder()
                            .child(&detail_view.widget)
                            .title(&album.title)
                            .build();

                        navigation_view_clone.push(&page);
                        hb_back.set_visible(true);
                        hb_search.set_visible(false);
                        hb_view.set_visible(false);
                        hb_widget.set_title_widget(Option::<&Widget>::None);
                    }
                    ArtistDetail(artist) => {
                        let detail_view = DetailView::builder()
                            .app_state(app_state_nav.clone())
                            .detail_type(ArtistDetailType(artist.clone()))
                            .compact(false)
                            .build();

                        let page = NavigationPage::builder()
                            .child(&detail_view.widget)
                            .title(&artist.name)
                            .build();

                        navigation_view_clone.push(&page);
                        hb_back.set_visible(true);
                        hb_search.set_visible(false);
                        hb_view.set_visible(false);
                        hb_widget.set_title_widget(Option::<&Widget>::None);
                    }
                }
            }
        }
    });

    // Tag the root page so we can pop back to it
    main_page.set_tag(Some("root"));

    // Sync AppState when NavigationView pops (e.g. via ESC or swipe)
    // We use a weak reference to AppState to avoid circular Arc leaks
    let app_state_weak = Arc::downgrade(app_state);
    navigation_view.connect_visible_page_notify(move |nv| {
        if let Some(page) = nv.visible_page()
            && page.tag().as_deref() == Some("root")
            && let Some(app_state) = app_state_weak.upgrade()
        {
            let current_nav = app_state.get_navigation_state();
            if current_nav != Library {
                debug!("NavigationView synced to root, updating AppState");
                app_state.update_navigation(Library);
            }
        }
    });

    // Create player bar
    let (player_bar_widget, _player_bar) =
        create_player_bar(app_state, audio_engine, queue_manager);

    // Subscribe to playback state changes to show/hide player bar
    let player_bar_widget_clone = player_bar_widget.clone();
    let app_state_for_subscription = app_state.clone();
    MainContext::default().spawn_local(async move {
        let receiver = app_state_for_subscription.subscribe();
        loop {
            if let Ok(event) = receiver.recv().await {
                if let PlaybackStateChanged(state) = event {
                    // Show player bar when a track is loaded, hide only when stopped
                    match state {
                        Playing | Paused | Buffering | Ready => {
                            player_bar_widget_clone.set_visible(true);
                        }
                        Stopped => {
                            player_bar_widget_clone.set_visible(false);
                        }
                    }
                }
            } else {
                // Channel was closed - resubscribe? Or just exit?
                // For async-channel manual fan-out, close usually means the sender is gone (AppState dropped).
                // So we should break.
                debug!("Playback state subscription channel closed");
                break;
            }
        }
    });

    // Assemble the main layout
    let main_box = GtkBox::builder().orientation(Vertical).build();

    main_box.append(&header_bar.widget);
    main_box.append(&navigation_view.upcast::<Widget>());
    main_box.append(&player_bar_widget);

    // Load custom CSS for consistent styling
    if let Err(e) = load_custom_css() {
        error!("Failed to load custom CSS: {e}");
    }

    // Set up ESC key shortcut for back navigation
    let app_state_esc = app_state.clone();
    let esc_controller = EventControllerKey::new();
    esc_controller.connect_key_pressed(move |_, key, _, _| {
        if key == Key::Escape {
            let current_nav = app_state_esc.get_navigation_state();
            if current_nav != Library {
                debug!("ESC pressed in detail view, navigating back to library");
                app_state_esc.update_navigation(Library);
                return Stop;
            }
        }
        Proceed
    });
    window.add_controller(esc_controller);

    // Set the window content
    window.set_content(Some(&main_box));
    window.present();
}

/// Creates the main content area with responsive layout.
fn create_main_content(
    app_state: &Arc<AppState>,
    settings_manager: &Arc<SettingsManager>,
    library_db: &Arc<LibraryDatabase>,
    audio_engine: &Arc<AudioEngine>,
    queue_manager: &Arc<QueueManager>,
    window: &ApplicationWindow,
    header_bar: &Rc<HeaderBar>,
) -> Widget {
    // Create stack for efficient view switching
    let view_stack = Stack::builder()
        .transition_type(Crossfade)
        .transition_duration(200)
        .build();

    let show_dr_badges = settings_manager.get_settings().show_dr_values;

    // Get current library state for view initialization
    let library_state = app_state.get_library_state();

    // Create all possible views upfront with individual scrolled windows
    let mut album_grid_view = AlbumGridView::builder()
        .app_state(app_state.clone())
        .library_db(library_db.clone())
        .audio_engine(audio_engine.clone())
        .queue_manager(queue_manager.clone())
        .albums(library_state.albums.clone())
        .show_dr_badges(show_dr_badges)
        .compact(false)
        .build();

    // Inject settings manager and window reference into empty state
    if let Some(empty_state) = &mut album_grid_view.empty_state {
        empty_state.settings_manager = Some((**settings_manager).clone());
        empty_state.window = Some(window.clone());
        empty_state.connect_button_handlers();
    }

    // Wrap album grid view in its own scrolled window
    let album_grid_scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&album_grid_view.widget)
        .build();

    let mut album_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Albums)
        .compact(false)
        .build();

    // Populate list view with initial data
    album_list_view.set_albums(library_state.albums.clone());

    // Wrap album list view in its own scrolled window
    let album_list_scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&album_list_view.widget)
        .build();

    let mut artist_grid_view = ArtistGridView::builder()
        .app_state(app_state.clone())
        .artists(library_state.artists.clone())
        .compact(false)
        .build();

    // Inject settings manager and window reference into empty state
    if let Some(empty_state) = &mut artist_grid_view.empty_state {
        empty_state.settings_manager = Some((**settings_manager).clone());
        empty_state.window = Some(window.clone());
        empty_state.connect_button_handlers();
    }

    // Wrap artist grid view in its own scrolled window
    let artist_grid_scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&artist_grid_view.widget)
        .build();

    let mut artist_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Artists)
        .compact(false)
        .build();

    // Populate list view with initial data
    artist_list_view.set_artists(library_state.artists.clone());

    // Wrap artist list view in its own scrolled window
    let artist_list_scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&artist_list_view.widget)
        .build();

    // Store view references in app state for dynamic access
    // This is a workaround since we can't easily pass mutable references
    // In a real implementation, we'd use a proper view manager

    // Get current state
    let library_state = app_state.get_library_state();
    let current_tab = library_state.current_tab;
    let current_view_mode = library_state.view_mode;

    // Add ALL scrolled views to the stack initially with unique names
    view_stack.add_named(&album_grid_scrolled, Some("album_grid"));
    view_stack.add_named(&album_list_scrolled, Some("album_list"));
    view_stack.add_named(&artist_grid_scrolled, Some("artist_grid"));
    view_stack.add_named(&artist_list_scrolled, Some("artist_list"));

    // Set initial visible view
    match (current_tab.clone(), current_view_mode.clone()) {
        (LibraryAlbums, Grid) => {
            view_stack.set_visible_child_name("album_grid");
        }
        (LibraryAlbums, List) => {
            view_stack.set_visible_child_name("album_list");
        }
        (LibraryArtists, Grid) => {
            view_stack.set_visible_child_name("artist_grid");
        }
        (LibraryArtists, List) => {
            view_stack.set_visible_child_name("artist_list");
        }
    }

    // Subscribe to state changes for view updates
    // Use tracing for monitoring
    debug!("Subscribing to AppState changes in main content");
    let app_state_clone = app_state.clone();
    let view_stack_clone = view_stack.clone();
    let header_bar_clone = Rc::clone(header_bar);
    let search_app_state = app_state.clone();

    // Use a weak reference to avoid potential memory leaks
    // and implement proper error handling for subscription
    MainContext::default().spawn_local(async move {
        let receiver = app_state_clone.subscribe();
        let mut switch_count = 0;
        let mut previous_tab = None;

        // Move view controllers into this closure to keep them alive and update them
        let mut album_grid_view = album_grid_view;
        let mut artist_grid_view = artist_grid_view;
        let mut album_list_view = album_list_view;
        let mut artist_list_view = artist_list_view;

        /// Applies a method or filter to the appropriate view based on child name.
        ///
        /// This macro dispatches operations to the correct view controller by matching
        /// the stack child name, eliminating repetitive match statements.
        ///
        /// # Patterns
        ///
        /// * `($child_name:expr, same, $method:ident)` - Invokes parameterless method
        /// * `($child_name:expr, same, $method:ident, $($arg:expr),*)` - Invokes method with args
        /// * `($child_name:expr, filter, $query:expr)` - Applies view-specific filter methods
        macro_rules! apply_to_views {
            ($child_name:expr, same, $method:ident) => {
                match $child_name {
                    "album_grid" => album_grid_view.$method(),
                    "album_list" => album_list_view.$method(),
                    "artist_grid" => artist_grid_view.$method(),
                    "artist_list" => artist_list_view.$method(),
                    _ => {}
                }
            };
            ($child_name:expr, same, $method:ident, $($arg:expr),*) => {
                match $child_name {
                    "album_grid" => album_grid_view.$method($($arg),*),
                    "album_list" => album_list_view.$method($($arg),*),
                    "artist_grid" => artist_grid_view.$method($($arg),*),
                    "artist_list" => artist_list_view.$method($($arg),*),
                    _ => {}
                }
            };
            ($child_name:expr, filter, $query:expr) => {
                match $child_name {
                    "album_grid" => album_grid_view.filter_albums($query),
                    "album_list" => album_list_view.filter_view_items($query),
                    "artist_grid" => artist_grid_view.filter_artists($query),
                    "artist_list" => artist_list_view.filter_view_items($query),
                    _ => {}
                }
            };
        }

        loop {
            if let Ok(event) = receiver.recv().await {
                match event {
                    LibraryDataChanged { albums, artists } => {
                        debug!("Handling LibraryDataChanged event");

                        // Save current search filter
                        let current_filter =
                            search_app_state.get_library_state().search_filter.clone();

                        // Update full lists
                        album_grid_view.update_all_albums(albums.clone());
                        artist_grid_view.update_all_artists(artists.clone());

                        // Update list views
                        album_list_view.set_albums(albums.clone());
                        artist_list_view.set_artists(artists.clone());

                        // Re-apply current search filter if active
                        if let Some(ref filter) = current_filter {
                            let query = filter.as_str();
                            let visible_child = view_stack_clone.visible_child_name();
                            if let Some(child_name) = visible_child.as_deref() {
                                apply_to_views!(child_name, filter, query);
                            }
                        }
                    }
                    ViewOptionsChanged {
                        current_tab,
                        view_mode,
                    } => {
                        switch_count += 1;
                        debug!(
                            "View switch #{}: tab={:?}, view_mode={:?}",
                            switch_count, current_tab, view_mode
                        );

                        // Check if tab changed (Albums ↔ Artists)
                        // Determine if tab changed - treat first switch (previous_tab is None) as a change too
                        let tab_changed = previous_tab.is_none_or(|prev| prev != current_tab);
                        previous_tab = Some(current_tab.clone());

                        let child_name = match (&current_tab, &view_mode) {
                            (LibraryAlbums, Grid) => "album_grid",
                            (LibraryAlbums, List) => "album_list",
                            (LibraryArtists, Grid) => "artist_grid",
                            (LibraryArtists, List) => "artist_list",
                        };

                        // Check if there's an active search filter
                        let library_state = search_app_state.get_library_state();
                        let has_active_search = library_state.search_filter.is_some();

                        // Handle view switching
                        if tab_changed {
                            // Tab switch: clear search filter to prevent stale results
                            debug!("Tab changed, resetting search filter");

                            if has_active_search {
                                debug!("Clearing target view before tab switch to prevent flicker");

                                apply_to_views!(child_name, same, clear_view);
                            } else {
                                // No active search - restore view to show all items
                                // (view may have been cleared in a previous switch with search)
                                debug!("Restoring view to show all items");

                                apply_to_views!(child_name, filter, "");
                            }
                        } else {
                            // View mode switch within same tab: preserve search results
                            debug!("View mode changed within same tab, preserving search");

                            if let Some(ref filter) = library_state.search_filter {
                                let query = filter.as_str();
                                debug!("Applying search filter '{query}' to new view {child_name}");

                                apply_to_views!(child_name, filter, query);
                            } else {
                                // No search - just show all items
                                apply_to_views!(child_name, filter, "");
                            }
                        }

                        // Reset scroll position before switching
                        if let Some(child) = view_stack_clone.child_by_name(child_name)
                            && let Some(scrolled) = child.downcast_ref::<ScrolledWindow>()
                        {
                            scrolled.vadjustment().set_value(0.0);
                            scrolled.hadjustment().set_value(0.0);
                        }

                        view_stack_clone.set_visible_child_name(child_name);

                        // Clear search AFTER view switch but WITHOUT broadcasting to prevent
                        // the outgoing view (still visible during crossfade) from showing all items
                        if tab_changed && has_active_search {
                            header_bar_clone.clear_search();
                            header_bar_clone.close_search();
                            search_app_state.clear_search_filter_silent();

                            // Restore the view to show all items now that the search is cleared
                            debug!("Restoring view after clearing search");

                            apply_to_views!(child_name, filter, "");
                        }
                    }
                    SearchFilterChanged(filter) => {
                        let query = filter.as_deref().unwrap_or("");

                        debug!("Updating search filter for all views: '{}'", query);

                        album_grid_view.filter_albums(query);
                        artist_grid_view.filter_artists(query);
                        album_list_view.filter_view_items(query);
                        artist_list_view.filter_view_items(query);
                    }
                    SettingsChanged { show_dr_values } => {
                        // Update DR badge visibility in all views
                        debug!(
                            "Handling SettingsChanged event: show_dr_values={}",
                            show_dr_values
                        );

                        // Update data in all views with new DR setting
                        album_grid_view.set_show_dr_badges(show_dr_values);

                        // Note: ListView doesn't currently have a set_show_dr_badges method,
                        // but it should respect the setting when creating new album rows
                    }
                    _ => {}
                }
            } else {
                debug!("Main view subscription channel closed");
                break;
            }
        }
    });

    view_stack.upcast::<Widget>()
}

/// Creates the persistent player control bar.
fn create_player_bar(
    app_state: &Arc<AppState>,
    audio_engine: &Arc<AudioEngine>,
    queue_manager: &Arc<QueueManager>,
) -> (GtkBox, PlayerBar) {
    let player_bar = PlayerBar::new(app_state, audio_engine, Some(queue_manager));
    let widget = player_bar.widget.clone();

    // Initially hide the player bar
    widget.set_visible(false);

    (widget.upcast::<GtkBox>(), player_bar)
}

/// Loads custom CSS for consistent component styling.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns an error if the CSS file cannot be read.
fn load_custom_css() -> Result<(), UiError> {
    let css_path = Path::new("data/style.css");
    let css = read_to_string(css_path).map_err(|e| {
        InitializationError(format!(
            "Failed to load CSS from {}: {e}",
            css_path.display()
        ))
    })?;

    let provider = CssProvider::new();
    provider.load_from_string(&css);
    style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Ok(())
}

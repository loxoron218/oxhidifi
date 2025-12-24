//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{error::Error, sync::Arc};

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
    tokio::sync::broadcast::error::RecvError::{Closed, Lagged},
    tracing::{debug, info},
};

use crate::{
    audio::engine::{
        AudioEngine,
        PlaybackState::{Buffering, Paused, Playing, Ready, Stopped},
    },
    config::SettingsManager,
    library::{
        LibraryDatabase,
        scanner::{LibraryScanner, ScannerEvent::LibraryChanged},
    },
    state::{
        AppState,
        AppStateEvent::{
            LibraryDataChanged, NavigationChanged, PlaybackStateChanged, SearchFilterChanged,
            ViewOptionsChanged,
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
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        // Initialize settings
        let settings =
            SettingsManager::new().map_err(|e| format!("Failed to initialize settings: {}", e))?;

        // Initialize audio engine
        let audio_engine =
            AudioEngine::new().map_err(|e| format!("Failed to initialize audio engine: {}", e))?;

        // Initialize library database
        let library_db_raw = LibraryDatabase::new()
            .await
            .map_err(|e| format!("Failed to initialize library database: {}", e))?;
        let library_db = Arc::new(library_db_raw);

        // Initialize library scanner if there are library directories
        let library_scanner = if !settings.get_settings().library_directories.is_empty() {
            let settings_arc = Arc::new(RwLock::new(settings.get_settings().clone()));
            let scanner = LibraryScanner::new(library_db.clone(), settings_arc.clone(), None)
                .await
                .map_err(|e| format!("Failed to initialize library scanner: {}", e))?;

            // Perform initial scan of existing directories
            if let Err(e) = scanner
                .scan_initial_directories(&library_db, &settings_arc)
                .await
            {
                eprintln!("Failed to perform initial library scan: {}", e);
            }

            Some(Arc::new(RwLock::new(scanner)))
        } else {
            None
        };

        // Create application state
        let app_state = AppState::new(
            Arc::downgrade(&Arc::new(audio_engine.clone())),
            library_scanner.clone(),
        );

        // Fetch initial library data and populate AppState
        if library_scanner.is_some() {
            let albums = match library_db.get_albums(None).await {
                Ok(albums) => albums,
                Err(e) => {
                    eprintln!("Failed to get albums from database: {}", e);
                    Vec::new()
                }
            };

            let artists = match library_db.get_artists(None).await {
                Ok(artists) => artists,
                Err(e) => {
                    eprintln!("Failed to get artists from database: {}", e);
                    Vec::new()
                }
            };

            // Update AppState with library data
            app_state.update_library_data(albums, artists);
        }

        let app = Application::builder()
            .application_id("com.example.oxhidifi")
            .build();

        Ok(OxhidifiApplication {
            app,
            audio_engine: Arc::new(audio_engine),
            library_db,
            library_scanner,
            app_state: Arc::new(app_state),
            settings: Arc::new(settings),
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

            move |_| {
                build_ui(
                    &app_clone,
                    &audio_engine_clone,
                    &library_db_clone,
                    &app_state_clone,
                    &settings_manager_clone,
                );

                // Subscribe to library scanner events if active
                if let Some(scanner_lock) = &app_state_clone.library_scanner.read().clone() {
                    let scanner = scanner_lock.read();
                    let mut rx = scanner.subscribe();
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
                                            eprintln!("Failed to refresh albums: {}", e);
                                            Vec::new()
                                        }
                                    };

                                    // Refresh artists
                                    let artists = match db_refresh.get_artists(None).await {
                                        Ok(artists) => artists,
                                        Err(e) => {
                                            eprintln!("Failed to refresh artists: {}", e);
                                            Vec::new()
                                        }
                                    };

                                    // Update state
                                    app_state_refresh.update_library_data(albums, artists);
                                }
                                Err(Closed) => {
                                    debug!("Scanner event channel closed");
                                    break;
                                }
                                Err(Lagged(skipped)) => {
                                    debug!(
                                        "Scanner event channel lagged, skipped {} events",
                                        skipped
                                    );
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
    let header_bar = HeaderBar::default_with_state(app_state.clone());

    // Create main content area with responsive layout
    let main_content = create_main_content(
        app_state,
        settings_manager,
        library_db,
        audio_engine,
        &window,
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
    let hb_view = header_bar.view_toggle.clone();
    let hb_tabs = header_bar.tab_box.clone();

    MainContext::default().spawn_local(async move {
        let mut receiver = app_state_nav.subscribe();
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let NavigationChanged(nav_state) = event {
                        match nav_state {
                            Library => {
                                let is_at_root =
                                    navigation_view_clone.visible_page().and_then(|p| p.tag())
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
                Err(Closed) => break,
                Err(Lagged(_)) => continue,
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
    let (player_bar_widget, _player_bar) = create_player_bar(app_state, audio_engine);

    // Subscribe to playback state changes to show/hide player bar
    let player_bar_widget_clone = player_bar_widget.clone();
    let app_state_for_subscription = app_state.clone();
    MainContext::default().spawn_local(async move {
        let mut receiver = app_state_for_subscription.subscribe();
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let PlaybackStateChanged(state) = event {
                        // Show player bar when playing or paused, hide when stopped
                        match state {
                            Playing | Paused | Buffering => {
                                player_bar_widget_clone.set_visible(true);
                            }
                            Stopped | Ready => {
                                player_bar_widget_clone.set_visible(false);
                            }
                        }
                    }
                }
                Err(Closed) => {
                    // Channel was closed - resubscribe
                    debug!("Playback state subscription channel closed, resubscribing");
                    receiver = app_state_for_subscription.subscribe();
                    continue;
                }
                Err(Lagged(skipped)) => {
                    debug!(
                        "Playback state subscription lagged, skipped {} messages",
                        skipped
                    );
                    continue;
                }
            }
        }
    });

    // Assemble the main layout
    let main_box = GtkBox::builder().orientation(Vertical).build();

    main_box.append(&header_bar.widget);
    main_box.append(&navigation_view.upcast::<Widget>());
    main_box.append(&player_bar_widget);

    // Load custom CSS for consistent styling
    load_custom_css();

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
    _library_db: &Arc<LibraryDatabase>,
    _audio_engine: &Arc<AudioEngine>,
    window: &ApplicationWindow,
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

    // Use a weak reference to avoid potential memory leaks
    // and implement proper error handling for subscription
    MainContext::default().spawn_local(async move {
        let mut receiver = app_state_clone.subscribe();
        let mut switch_count = 0;

        // Move view controllers into this closure to keep them alive and update them
        let mut album_grid_view = album_grid_view;
        let mut artist_grid_view = artist_grid_view;
        let mut album_list_view = album_list_view;
        let mut artist_list_view = artist_list_view;

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    match event {
                        LibraryDataChanged { albums, artists } => {
                            debug!("Handling LibraryDataChanged event");

                            // Update data in all views
                            album_grid_view.set_albums(albums.clone());
                            artist_grid_view.set_artists(artists.clone());
                            album_list_view.set_albums(albums.clone());
                            artist_list_view.set_artists(artists.clone());
                        }
                        ViewOptionsChanged {
                            current_tab,
                            view_mode,
                        } => {
                            switch_count += 1;
                            info!(
                                "View switch #{}: tab={:?}, view_mode={:?}",
                                switch_count, current_tab, view_mode
                            );

                            // Switch to the appropriate view based on state
                            match (current_tab, view_mode) {
                                (LibraryAlbums, Grid) => {
                                    // Reset scroll position before switching
                                    if let Some(child) =
                                        view_stack_clone.child_by_name("album_grid")
                                        && let Some(scrolled) =
                                            child.downcast_ref::<ScrolledWindow>()
                                    {
                                        scrolled.vadjustment().set_value(0.0);
                                        scrolled.hadjustment().set_value(0.0);
                                    }
                                    view_stack_clone.set_visible_child_name("album_grid");
                                }
                                (LibraryAlbums, List) => {
                                    // Reset scroll position before switching
                                    if let Some(child) =
                                        view_stack_clone.child_by_name("album_list")
                                        && let Some(scrolled) =
                                            child.downcast_ref::<ScrolledWindow>()
                                    {
                                        scrolled.vadjustment().set_value(0.0);
                                        scrolled.hadjustment().set_value(0.0);
                                    }
                                    view_stack_clone.set_visible_child_name("album_list");
                                }
                                (LibraryArtists, Grid) => {
                                    // Reset scroll position before switching
                                    if let Some(child) =
                                        view_stack_clone.child_by_name("artist_grid")
                                        && let Some(scrolled) =
                                            child.downcast_ref::<ScrolledWindow>()
                                    {
                                        scrolled.vadjustment().set_value(0.0);
                                        scrolled.hadjustment().set_value(0.0);
                                    }
                                    view_stack_clone.set_visible_child_name("artist_grid");
                                }
                                (LibraryArtists, List) => {
                                    // Reset scroll position before switching
                                    if let Some(child) =
                                        view_stack_clone.child_by_name("artist_list")
                                        && let Some(scrolled) =
                                            child.downcast_ref::<ScrolledWindow>()
                                    {
                                        scrolled.vadjustment().set_value(0.0);
                                        scrolled.hadjustment().set_value(0.0);
                                    }
                                    view_stack_clone.set_visible_child_name("artist_list");
                                }
                            }
                        }
                        SearchFilterChanged(filter) => {
                            // Search filter changed - update all views
                            // Note: Each view handles filtering internally, we just need to pass the query
                            let query = filter.as_deref().unwrap_or("");

                            debug!("Updating search filter for all views: '{}'", query);

                            album_grid_view.filter_albums(query);
                            artist_grid_view.filter_artists(query);
                            album_list_view.filter_items(query);
                            artist_list_view.filter_items(query);
                        }
                        _ => {}
                    }
                }
                Err(Closed) => {
                    // Channel was closed - this can happen when all receivers are dropped
                    // and the AppState recreates the channel. We should resubscribe.
                    debug!("State subscription channel closed, attempting to resubscribe");
                    receiver = app_state_clone.subscribe();
                    continue;
                }
                Err(Lagged(skipped)) => {
                    // Receiver lagged behind, but this is not critical
                    // The receiver will get the next event
                    debug!(
                        "State subscription lagged behind, skipped {} messages",
                        skipped
                    );
                    continue;
                }
            }
        }
    });

    view_stack.upcast::<Widget>()
}

/// Creates the persistent player control bar.
fn create_player_bar(
    app_state: &Arc<AppState>,
    audio_engine: &Arc<AudioEngine>,
) -> (GtkBox, PlayerBar) {
    let player_bar = PlayerBar::new(app_state.clone(), audio_engine.clone());
    let widget = player_bar.widget.clone();

    // Initially hide the player bar
    widget.set_visible(false);

    (widget.upcast::<GtkBox>(), player_bar)
}

/// Loads custom CSS for consistent component styling.
fn load_custom_css() {
    // Define CSS for DR badges and cover art components
    let css = r#"
        /* Cover art container styling */
        .cover-art-container {
            background-color: @theme_bg_color;
            border-radius: 6px;
        }
        
        /* Cover art picture styling */
        .cover-art-picture {
            border-radius: 6px;
            background-color: @theme_unfocused_bg_color;
        }
        
        /* DR badge styling - consistent 28x28px with proper positioning */
        .dr-badge-label {
            font-family: monospace;
            font-weight: bold;
            font-size: 12px;
            min-width: 28px;
            min-height: 28px;
            border-radius: 4px;
            padding: 0;
            margin: 4px; /* Small margin to prevent touching edges */
            color: white;
            box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
        }
        
        /* DR badge quality colors */
        .dr-badge-label.dr-14,
        .dr-badge-label.dr-15 { background-color: #4CAF50; }    /* Green - Excellent */
        .dr-badge-label.dr-12,
        .dr-badge-label.dr-13 { background-color: #8BC34A; }    /* Light Green - Good */
        .dr-badge-label.dr-10,
        .dr-badge-label.dr-11 { background-color: #FFC107; }    /* Amber - Fair */
        .dr-badge-label.dr-08,
        .dr-badge-label.dr-09 { background-color: #FF9800; }    /* Orange - Poor */
        .dr-badge-label.dr-00,
        .dr-badge-label.dr-01,
        .dr-badge-label.dr-02,
        .dr-badge-label.dr-03,
        .dr-badge-label.dr-04,
        .dr-badge-label.dr-05,
        .dr-badge-label.dr-06,
        .dr-badge-label.dr-07 { background-color: #F44336; }    /* Red - Very Poor */
        .dr-badge-label.dr-na { background-color: #9E9E9E; }    /* Gray - Unknown */
        
        /* Individual DR value classes for precise styling */
        .dr-badge-label.dr-00 { background-color: #F44336; }
        .dr-badge-label.dr-01 { background-color: #F44336; }
        .dr-badge-label.dr-02 { background-color: #F44336; }
        .dr-badge-label.dr-03 { background-color: #F44336; }
        .dr-badge-label.dr-04 { background-color: #F44336; }
        .dr-badge-label.dr-05 { background-color: #F44336; }
        .dr-badge-label.dr-06 { background-color: #F44336; }
        .dr-badge-label.dr-07 { background-color: #F44336; }
        .dr-badge-label.dr-08 { background-color: #FF9800; }
        .dr-badge-label.dr-09 { background-color: #FF9800; }
        .dr-badge-label.dr-10 { background-color: #FFC107; }
        .dr-badge-label.dr-11 { background-color: #FFC107; }
        .dr-badge-label.dr-12 { background-color: #8BC34A; }
        .dr-badge-label.dr-13 { background-color: #8BC34A; }
        .dr-badge-label.dr-14 { background-color: #4CAF50; }
        .dr-badge-label.dr-15 { background-color: #4CAF50; }
        
        /* Album card base styling */
        .album-tile {
            background-color: transparent;
            border-radius: 12px; /* 12px border-radius as specified */
            min-width: 64px; /* Minimum width for smallest zoom level */
        }
                
        /* Album title label styling */
        .album-title-label {
            font-weight: 700; /* Bold 1.1em font-weight: 700 */
            font-size: 1.1em;
        }
        
        /* Album artist label styling */
        .album-artist-label {
            color: #AAA; /* Light gray (#AAA) */
            font-weight: 400;
            font-size: 0.95em;
        }
        
        /* Album format/genre label styling */
        .album-format-label {
            color: #666; /* Darker gray (#666) */
            font-style: italic;
            font-size: 0.9em;
            margin-top: 2px;
        }
        
        /* Play overlay styling */
        .play-overlay {
            background-color: rgba(0, 0, 0, 0.6);
            border-radius: 50%;
            min-width: 40px;
            min-height: 40px;
            opacity: 0;
            transition: opacity 0.2s ease-in-out;
        }
        
        .play-overlay:hover {
            background-color: rgba(0, 0, 0, 0.9);
        }
        
        /* Album grid styling */
        .album-grid {
            /* FlowBox will handle the responsive grid layout */
            min-width: 360px; /* Minimum width for mobile-like displays */
        }
    "#;

    let provider = CssProvider::new();
    provider.load_from_string(css);
    style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

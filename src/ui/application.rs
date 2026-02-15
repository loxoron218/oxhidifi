//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{fs::read_to_string, path::Path, rc::Rc, sync::Arc};

use {
    libadwaita::{
        Application, ApplicationWindow, NavigationPage, NavigationView, Toast, ToastOverlay,
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
        models::{Album, Artist},
        scanner::{LibraryScanner, ScannerEvent::LibraryChanged},
    },
    state::{
        AppState,
        AppStateEvent::{
            ExclusiveModeFailed, LibraryDataChanged, NavigationChanged, PlaybackStateChanged,
            SearchFilterChanged, SettingsChanged, ViewOptionsChanged,
        },
        NavigationState::{AlbumDetail, ArtistDetail, Library},
        ViewMode::{self, Grid, List},
        app_state::{
            LibraryState,
            LibraryTab::{self, Albums as LibraryAlbums, Artists as LibraryArtists},
        },
    },
    ui::{
        header_bar::HeaderBar,
        player_bar::PlayerBar,
        views::{
            AlbumGridView, ArtistGridView,
            DetailType::{Album as DetailTypeAlbum, Artist as DetailTypeArtist},
            DetailView, ListView,
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

/// Context struct holding all view controllers.
struct ViewControllers {
    /// Album grid view controller.
    album_grid: AlbumGridView,
    /// Album list view controller.
    album_list: ListView,
    /// Artist grid view controller.
    artist_grid: ArtistGridView,
    /// Artist list view controller.
    artist_list: ListView,
}

/// Context struct for view options handling.
struct ViewOptionsContext<'a> {
    /// The view stack widget.
    view_stack: &'a Stack,
    /// The header bar widget.
    header_bar: &'a Rc<HeaderBar>,
    /// The application state.
    app_state: &'a Arc<AppState>,
    /// Mutable reference to view controllers.
    views: &'a mut ViewControllers,
    /// Previous tab state for detecting tab changes.
    previous_tab: &'a mut Option<LibraryTab>,
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

        // Apply user settings to audio engine output config
        {
            let settings = settings_manager_shared.get_settings();
            let mut output_config = audio_engine.output_config();
            output_config.exclusive_mode = settings.exclusive_mode;
            output_config.sample_rate = settings.sample_rate;
            output_config.buffer_duration_ms = settings.buffer_duration_ms;
            audio_engine.update_output_config(output_config);
        }

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

        // Set up error callback for exclusive mode failures
        let app_state_for_error = app_state.clone();
        audio_engine.set_error_callback(move |error_msg: String| {
            app_state_for_error.report_exclusive_mode_failure(error_msg);
        });

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

        let albums = library_db
            .get_albums(None)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to get albums from database");
                e
            })
            .unwrap_or_default();

        let artists = library_db
            .get_artists(None)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to get artists from database");
                e
            })
            .unwrap_or_default();

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
                                    let albums = db_refresh
                                        .get_albums(None)
                                        .await
                                        .map_err(|e| {
                                            error!(error = %e, "Failed to refresh albums");
                                            e
                                        })
                                        .unwrap_or_default();

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

    // Create toast overlay for displaying error messages
    let toast_overlay = ToastOverlay::new();

    // Create header bar with proper state integration
    let header_bar = Rc::new(HeaderBar::default_with_state(
        app_state,
        app.clone(),
        settings_manager.clone(),
    ));

    // Create main content area with responsive layout
    let main_content = create_main_content(
        app_state,
        library_db,
        audio_engine,
        queue_manager,
        &window,
        &header_bar,
        &toast_overlay,
    );

    // Add main content as root page
    let main_page = NavigationPage::builder()
        .child(&main_content)
        .title("Main")
        .build();
    navigation_view.add(&main_page);

    // Set navigation view as child of toast overlay
    toast_overlay.set_child(Some(&navigation_view));

    // Handle navigation events and update HeaderBar state centrally
    let navigation_view_clone = navigation_view.clone();
    let toast_overlay_clone = toast_overlay.clone();
    let app_state_nav = app_state.clone();
    let library_db_nav = library_db.clone();
    let audio_engine_nav = audio_engine.clone();
    let queue_manager_nav = queue_manager.clone();
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
                        let detail_view = match DetailView::builder()
                            .app_state(app_state_nav.clone())
                            .library_db(library_db_nav.clone())
                            .audio_engine(audio_engine_nav.clone())
                            .queue_manager(queue_manager_nav.clone())
                            .detail_type(Some(DetailTypeAlbum(album.clone())))
                            .compact(false)
                            .build()
                        {
                            Ok(view) => view,
                            Err(e) => {
                                error!("Failed to build album detail view: {e}");
                                let toast = Toast::new("Failed to load album details");
                                toast_overlay_clone.add_toast(toast);
                                continue;
                            }
                        };

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
                        let detail_view = match DetailView::builder()
                            .app_state(app_state_nav.clone())
                            .library_db(library_db_nav.clone())
                            .audio_engine(audio_engine_nav.clone())
                            .queue_manager(queue_manager_nav.clone())
                            .detail_type(Some(DetailTypeArtist(artist.clone())))
                            .compact(false)
                            .build()
                        {
                            Ok(view) => view,
                            Err(e) => {
                                error!("Failed to build artist detail view: {e}");
                                let toast = Toast::new("Failed to load artist details");
                                toast_overlay_clone.add_toast(toast);
                                continue;
                            }
                        };

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
    let (player_bar_widget, player_bar) = create_player_bar(app_state, audio_engine, queue_manager);
    let player_bar = Rc::new(player_bar);
    let player_bar_for_keepalive = player_bar.clone();

    // Subscribe to playback state changes to show/hide player bar
    let player_bar_widget_clone = player_bar_widget.clone();
    let app_state_for_subscription = app_state.clone();

    MainContext::default().spawn_local(async move {
        // Keep player_bar alive throughout the subscription closure lifetime
        let _player_bar_keepalive = player_bar_for_keepalive;

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
    main_box.append(&toast_overlay);
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

/// Creates the view stack widget with smooth transitions.
///
/// # Returns
///
/// A configured `Stack` widget ready for child addition.
fn create_view_stack() -> Stack {
    Stack::builder()
        .transition_type(Crossfade)
        .transition_duration(200)
        .build()
}

/// Wraps a widget in a scrolled window with consistent margins.
///
/// # Arguments
///
/// * `child` - The widget to wrap
///
/// # Returns
///
/// A `ScrolledWindow` containing the child widget with 12px margins.
fn create_scrolled_window(child: &Widget) -> ScrolledWindow {
    ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(child)
        .build()
}

/// Creates album grid view with empty state handlers.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `library_db` - Library database reference
/// * `audio_engine` - Audio engine reference
/// * `queue_manager` - Queue manager reference
/// * `library_state` - Current library state
/// * `show_dr_badges` - Whether to show DR value badges
/// * `window` - Application window for empty state button handlers
///
/// # Returns
///
/// A tuple of the `AlbumGridView` and its wrapped `ScrolledWindow`.
fn create_album_grid_view(
    app_state: &Arc<AppState>,
    library_db: &Arc<LibraryDatabase>,
    audio_engine: &Arc<AudioEngine>,
    queue_manager: &Arc<QueueManager>,
    library_state: &LibraryState,
    show_dr_badges: bool,
    window: &ApplicationWindow,
) -> (AlbumGridView, ScrolledWindow) {
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
        empty_state.settings_manager = Some(app_state.settings_manager.read().clone());
        empty_state.window = Some(window.clone());
        empty_state.connect_button_handlers();
    }

    // Wrap album grid view in its own scrolled window
    let scrolled = create_scrolled_window(&album_grid_view.widget);

    (album_grid_view, scrolled)
}

/// Creates album list view with initial data.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `library_state` - Current library state
///
/// # Returns
///
/// A tuple of the `ListView` and its wrapped `ScrolledWindow`.
fn create_album_list_view(
    app_state: &Arc<AppState>,
    library_state: &LibraryState,
) -> (ListView, ScrolledWindow) {
    let mut album_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Albums)
        .compact(false)
        .build();

    // Populate list view with initial data
    album_list_view.set_albums(library_state.albums.clone());

    let scrolled = create_scrolled_window(&album_list_view.widget);

    (album_list_view, scrolled)
}

/// Creates artist grid view with empty state handlers.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `library_state` - Current library state
/// * `window` - Application window for empty state button handlers
///
/// # Returns
///
/// A tuple of the `ArtistGridView` and its wrapped `ScrolledWindow`.
fn create_artist_grid_view(
    app_state: &Arc<AppState>,
    library_state: &LibraryState,
    window: &ApplicationWindow,
) -> (ArtistGridView, ScrolledWindow) {
    let mut artist_grid_view = ArtistGridView::builder()
        .app_state(app_state.clone())
        .artists(library_state.artists.clone())
        .compact(false)
        .build();

    // Inject settings manager and window reference into empty state
    if let Some(empty_state) = &mut artist_grid_view.empty_state {
        empty_state.settings_manager = Some(app_state.settings_manager.read().clone());
        empty_state.window = Some(window.clone());
        empty_state.connect_button_handlers();
    }

    let scrolled = create_scrolled_window(&artist_grid_view.widget);

    (artist_grid_view, scrolled)
}

/// Creates artist list view with initial data.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `library_state` - Current library state
///
/// # Returns
///
/// A tuple of the `ListView` and its wrapped `ScrolledWindow`.
fn create_artist_list_view(
    app_state: &Arc<AppState>,
    library_state: &LibraryState,
) -> (ListView, ScrolledWindow) {
    let mut artist_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Artists)
        .compact(false)
        .build();

    // Populate list view with initial data
    artist_list_view.set_artists(library_state.artists.clone());

    let scrolled = create_scrolled_window(&artist_list_view.widget);

    (artist_list_view, scrolled)
}

/// Adds all views to the stack with unique names.
///
/// # Arguments
///
/// * `view_stack` - The stack widget to add views to
/// * `album_grid_scrolled` - Album grid view wrapper
/// * `album_list_scrolled` - Album list view wrapper
/// * `artist_grid_scrolled` - Artist grid view wrapper
/// * `artist_list_scrolled` - Artist list view wrapper
fn add_views_to_stack(
    view_stack: &Stack,
    album_grid_scrolled: &ScrolledWindow,
    album_list_scrolled: &ScrolledWindow,
    artist_grid_scrolled: &ScrolledWindow,
    artist_list_scrolled: &ScrolledWindow,
) {
    // Add ALL scrolled views to the stack initially with unique names
    view_stack.add_named(album_grid_scrolled, Some("album_grid"));
    view_stack.add_named(album_list_scrolled, Some("album_list"));
    view_stack.add_named(artist_grid_scrolled, Some("artist_grid"));
    view_stack.add_named(artist_list_scrolled, Some("artist_list"));
}

/// Sets the initially visible view based on current tab and mode.
///
/// # Arguments
///
/// * `view_stack` - The stack widget
/// * `current_tab` - Current library tab
/// * `current_view_mode` - Current view mode
fn set_initial_visible_view(
    view_stack: &Stack,
    current_tab: &LibraryTab,
    current_view_mode: &ViewMode,
) {
    let child_name = match (current_tab, current_view_mode) {
        (LibraryAlbums, Grid) => "album_grid",
        (LibraryAlbums, List) => "album_list",
        (LibraryArtists, Grid) => "artist_grid",
        (LibraryArtists, List) => "artist_list",
    };

    view_stack.set_visible_child_name(child_name);
}

/// Handles library data changed events.
///
/// # Arguments
///
/// * `albums` - Updated album list
/// * `artists` - Updated artist list
/// * `views` - View controllers context
/// * `search_app_state` - `AppState` for retrieving search filter
/// * `view_stack` - View stack for getting current child
fn handle_library_data_changed(
    albums: &[Album],
    artists: &[Artist],
    views: &mut ViewControllers,
    search_app_state: &Arc<AppState>,
    view_stack: &Stack,
) {
    debug!("Handling LibraryDataChanged event");

    // Save current search filter
    let current_filter = search_app_state.get_library_state().search_filter.clone();

    // Update full lists
    views.album_grid.update_all_albums(albums.to_vec());
    views.artist_grid.update_all_artists(artists.to_vec());

    // Update list views
    views.album_list.set_albums(albums.to_vec());
    views.artist_list.set_artists(artists.to_vec());

    // Re-apply current search filter if active
    if let Some(ref filter) = current_filter {
        let query = filter.as_str();
        let visible_child = view_stack.visible_child_name();
        if let Some(child_name) = visible_child.as_deref() {
            match child_name {
                "album_grid" => views.album_grid.filter_albums(query),
                "album_list" => views.album_list.filter_view_items(query),
                "artist_grid" => views.artist_grid.filter_artists(query),
                "artist_list" => views.artist_list.filter_view_items(query),
                _ => {}
            }
        }
    }
}

/// Handles view options changed events (tab and view mode switches).
///
/// # Arguments
///
/// * `current_tab` - New current tab
/// * `view_mode` - New view mode
/// * `ctx` - View options context containing all necessary references
fn handle_view_options_changed(
    current_tab: &LibraryTab,
    view_mode: &ViewMode,
    ctx: &mut ViewOptionsContext<'_>,
) {
    // Determine if tab changed - treat first switch (previous_tab is None) as a change too
    let tab_changed = ctx
        .previous_tab
        .clone()
        .is_none_or(|prev| prev != *current_tab);
    *ctx.previous_tab = Some(current_tab.clone());

    let child_name = match (current_tab, view_mode) {
        (LibraryAlbums, Grid) => "album_grid",
        (LibraryAlbums, List) => "album_list",
        (LibraryArtists, Grid) => "artist_grid",
        (LibraryArtists, List) => "artist_list",
    };

    // Check if there's an active search filter
    let library_state = ctx.app_state.get_library_state();
    let has_active_search = library_state.search_filter.is_some();

    // Handle view switching
    if tab_changed {
        // Tab switch: clear search filter to prevent stale results
        debug!("Tab changed, resetting search filter");

        if has_active_search {
            debug!("Clearing target view before tab switch to prevent flicker");

            match child_name {
                "album_grid" => ctx.views.album_grid.clear_view(),
                "album_list" => ctx.views.album_list.clear_view(),
                "artist_grid" => ctx.views.artist_grid.clear_view(),
                "artist_list" => ctx.views.artist_list.clear_view(),
                _ => {}
            }
        } else {
            // No active search - restore view to show all items
            debug!("Restoring view to show all items");

            match child_name {
                "album_grid" => ctx.views.album_grid.filter_albums(""),
                "album_list" => ctx.views.album_list.filter_view_items(""),
                "artist_grid" => ctx.views.artist_grid.filter_artists(""),
                "artist_list" => ctx.views.artist_list.filter_view_items(""),
                _ => {}
            }
        }
    } else {
        // View mode switch within same tab: preserve search results
        debug!("View mode changed within same tab, preserving search");

        if let Some(ref filter) = library_state.search_filter {
            let query = filter.as_str();
            debug!("Applying search filter '{query}' to new view {child_name}");

            match child_name {
                "album_grid" => ctx.views.album_grid.filter_albums(query),
                "album_list" => ctx.views.album_list.filter_view_items(query),
                "artist_grid" => ctx.views.artist_grid.filter_artists(query),
                "artist_list" => ctx.views.artist_list.filter_view_items(query),
                _ => {}
            }
        } else {
            // No search - just show all items
            match child_name {
                "album_grid" => ctx.views.album_grid.filter_albums(""),
                "album_list" => ctx.views.album_list.filter_view_items(""),
                "artist_grid" => ctx.views.artist_grid.filter_artists(""),
                "artist_list" => ctx.views.artist_list.filter_view_items(""),
                _ => {}
            }
        }
    }

    // Reset scroll position before switching
    if let Some(child) = ctx.view_stack.child_by_name(child_name)
        && let Some(scrolled) = child.downcast_ref::<ScrolledWindow>()
    {
        scrolled.vadjustment().set_value(0.0);
        scrolled.hadjustment().set_value(0.0);
    }

    ctx.view_stack.set_visible_child_name(child_name);

    // Clear search AFTER view switch but WITHOUT broadcasting to prevent
    // the outgoing view (still visible during crossfade) from showing all items
    if tab_changed && has_active_search {
        ctx.header_bar.clear_search();
        ctx.header_bar.close_search();
        ctx.app_state.clear_search_filter_silent();

        // Restore the view to show all items now that the search is cleared
        debug!("Restoring view after clearing search");

        match child_name {
            "album_grid" => ctx.views.album_grid.filter_albums(""),
            "album_list" => ctx.views.album_list.filter_view_items(""),
            "artist_grid" => ctx.views.artist_grid.filter_artists(""),
            "artist_list" => ctx.views.artist_list.filter_view_items(""),
            _ => {}
        }
    }
}

/// Handles search filter changed events.
///
/// # Arguments
///
/// * `filter` - Optional search filter query
/// * `views` - View controllers context
fn handle_search_filter_changed(filter: Option<&str>, views: &mut ViewControllers) {
    let query = filter.unwrap_or("");

    debug!("Updating search filter for all views: '{}'", query);

    views.album_grid.filter_albums(query);
    views.artist_grid.filter_artists(query);
    views.album_list.filter_view_items(query);
    views.artist_list.filter_view_items(query);
}

/// Handles settings changed events.
///
/// # Arguments
///
/// * `show_dr_values` - Whether to show DR value badges
/// * `views` - View controllers context
fn handle_settings_changed(show_dr_values: bool, views: &mut ViewControllers) {
    debug!(
        "Handling SettingsChanged event: show_dr_values={}",
        show_dr_values
    );

    views.album_grid.set_show_dr_badges(show_dr_values);

    // Note: ListView doesn't currently have a set_show_dr_badges method,
    // but it should respect the setting when creating new album rows
}

/// Handles exclusive mode failed events.
///
/// # Arguments
///
/// * `reason` - Reason for the failure
/// * `toast_overlay` - Toast overlay for displaying errors
fn handle_exclusive_mode_failed(reason: &str, toast_overlay: &ToastOverlay) {
    debug!("Handling ExclusiveModeFailed event: reason='{}'", reason);

    let toast = Toast::new(&format!("Exclusive mode playback failed: {reason}"));
    toast_overlay.add_toast(toast);
}

/// Spawns async event handler for view stack state changes.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `view_stack` - View stack to update
/// * `header_bar` - Header bar for search control
/// * `toast_overlay` - Toast overlay for error display
/// * `views` - View controllers
fn spawn_view_stack_event_handler(
    app_state: Arc<AppState>,
    view_stack: Stack,
    header_bar: Rc<HeaderBar>,
    toast_overlay: ToastOverlay,
    views: ViewControllers,
) {
    debug!("Subscribing to AppState changes in main content");

    MainContext::default().spawn_local(async move {
        let receiver = app_state.subscribe();
        let mut switch_count = 0;
        let mut previous_tab = None;
        let mut views = views;
        let search_app_state = app_state.clone();

        loop {
            if let Ok(event) = receiver.recv().await {
                switch_count += 1;

                match event {
                    LibraryDataChanged { albums, artists } => {
                        handle_library_data_changed(
                            &albums,
                            &artists,
                            &mut views,
                            &search_app_state,
                            &view_stack,
                        );
                    }
                    ViewOptionsChanged {
                        current_tab,
                        view_mode,
                    } => {
                        debug!(
                            "View switch #{}: tab={:?}, view_mode={:?}",
                            switch_count, current_tab, view_mode
                        );

                        let mut ctx = ViewOptionsContext {
                            view_stack: &view_stack,
                            header_bar: &header_bar,
                            app_state: &search_app_state,
                            views: &mut views,
                            previous_tab: &mut previous_tab,
                        };

                        handle_view_options_changed(&current_tab, &view_mode, &mut ctx);
                    }
                    SearchFilterChanged(filter) => {
                        handle_search_filter_changed(filter.as_deref(), &mut views);
                    }
                    SettingsChanged { show_dr_values } => {
                        handle_settings_changed(show_dr_values, &mut views);
                    }
                    ExclusiveModeFailed { reason } => {
                        handle_exclusive_mode_failed(&reason, &toast_overlay);
                    }
                    _ => {}
                }
            } else {
                debug!("Main view subscription channel closed");
                break;
            }
        }
    });
}

/// Creates the main content area with responsive layout.
///
/// # Arguments
///
/// * `app_state` - Application state reference
/// * `library_db` - Library database reference
/// * `audio_engine` - Audio engine reference
/// * `queue_manager` - Queue manager reference
/// * `window` - Application window reference
/// * `header_bar` - Header bar reference
/// * `toast_overlay` - Toast overlay reference
///
/// # Returns
///
/// The main content widget containing all library views.
fn create_main_content(
    app_state: &Arc<AppState>,
    library_db: &Arc<LibraryDatabase>,
    audio_engine: &Arc<AudioEngine>,
    queue_manager: &Arc<QueueManager>,
    window: &ApplicationWindow,
    header_bar: &Rc<HeaderBar>,
    toast_overlay: &ToastOverlay,
) -> Widget {
    let view_stack = create_view_stack();

    let show_dr_badges = app_state
        .settings_manager
        .read()
        .get_settings()
        .show_dr_values;

    let library_state = app_state.get_library_state();

    let (album_grid_view, album_grid_scrolled) = create_album_grid_view(
        app_state,
        library_db,
        audio_engine,
        queue_manager,
        &library_state,
        show_dr_badges,
        window,
    );

    let (album_list_view, album_list_scrolled) = create_album_list_view(app_state, &library_state);

    let (artist_grid_view, artist_grid_scrolled) =
        create_artist_grid_view(app_state, &library_state, window);

    let (artist_list_view, artist_list_scrolled) =
        create_artist_list_view(app_state, &library_state);

    add_views_to_stack(
        &view_stack,
        &album_grid_scrolled,
        &album_list_scrolled,
        &artist_grid_scrolled,
        &artist_list_scrolled,
    );

    let current_tab = library_state.current_tab;
    let current_view_mode = library_state.view_mode;

    set_initial_visible_view(&view_stack, &current_tab, &current_view_mode);

    let views = ViewControllers {
        album_grid: album_grid_view,
        album_list: album_list_view,
        artist_grid: artist_grid_view,
        artist_list: artist_list_view,
    };

    spawn_view_stack_event_handler(
        Arc::clone(app_state),
        view_stack.clone(),
        Rc::clone(header_bar),
        toast_overlay.clone(),
        views,
    );

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
    let display = Display::default()
        .ok_or_else(|| InitializationError("Could not connect to a display".into()))?;
    style_context_add_provider_for_display(
        &display,
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Ok(())
}

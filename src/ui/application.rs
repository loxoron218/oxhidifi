//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{error::Error, sync::Arc};

use {
    libadwaita::{
        Application, ApplicationWindow, NavigationPage, NavigationView,
        glib::MainContext,
        gtk::{
            Box as GtkBox, Orientation::Vertical, ScrolledWindow, Stack,
            StackTransitionType::Crossfade, Widget,
        },
        prelude::{
            AdwApplicationWindowExt, ApplicationExt, ApplicationExtManual, BoxExt, Cast,
            GtkWindowExt, WidgetExt,
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
    library::{LibraryDatabase, scanner::LibraryScanner},
    state::{
        AppState,
        AppStateEvent::{LibraryStateChanged, PlaybackStateChanged, SearchFilterChanged},
        ViewMode::{Grid, List},
        app_state::LibraryTab::{Albums as LibraryAlbums, Artists as LibraryArtists},
    },
    ui::{
        header_bar::HeaderBar,
        player_bar::PlayerBar,
        views::{
            AlbumGridView, ArtistGridView, ListView,
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
            let mut library_state = app_state.get_library_state();
            library_state.albums = albums;
            library_state.artists = artists;
            app_state.update_library_state(library_state);
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

    // Store navigation view reference for detail view navigation
    // In a real implementation, this would be stored in AppState or a navigation manager

    // Create header bar with proper state integration
    let header_bar = HeaderBar::default_with_state(app_state.clone());

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

    // Wrap the stack in a scrolled window to provide vertical scrolling
    let scrolled_window = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&view_stack)
        .build();

    let show_dr_badges = settings_manager.get_settings().show_dr_values;

    // Get current library state for view initialization
    let library_state = app_state.get_library_state();

    // Create all possible views upfront
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

    let mut album_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Albums)
        .compact(false)
        .build();

    // Populate list view with initial data
    album_list_view.set_albums(library_state.albums.clone());

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

    let mut artist_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Artists)
        .compact(false)
        .build();

    // Populate list view with initial data
    artist_list_view.set_artists(library_state.artists.clone());

    // Store view references in app state for dynamic access
    // This is a workaround since we can't easily pass mutable references
    // In a real implementation, we'd use a proper view manager

    // Get current state
    let library_state = app_state.get_library_state();
    let current_tab = library_state.current_tab;
    let current_view_mode = library_state.view_mode;

    // Add ALL views to the stack initially with unique names
    view_stack.add_named(&album_grid_view.widget, Some("album_grid"));
    view_stack.add_named(&album_list_view.widget, Some("album_list"));
    view_stack.add_named(&artist_grid_view.widget, Some("artist_grid"));
    view_stack.add_named(&artist_list_view.widget, Some("artist_list"));

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

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    match event {
                        LibraryStateChanged(new_state) => {
                            switch_count += 1;
                            info!(
                                "View switch #{}: tab={:?}, view_mode={:?}",
                                switch_count, new_state.current_tab, new_state.view_mode
                            );

                            // Add debug logging for performance monitoring
                            if switch_count % 10 == 0 {
                                debug!("Performance check - view switches: {}", switch_count);
                            }

                            // Switch to the appropriate view based on state
                            match (new_state.current_tab, new_state.view_mode) {
                                (LibraryAlbums, Grid) => {
                                    view_stack_clone.set_visible_child_name("album_grid");
                                }
                                (LibraryAlbums, List) => {
                                    view_stack_clone.set_visible_child_name("album_list");
                                }
                                (LibraryArtists, Grid) => {
                                    view_stack_clone.set_visible_child_name("artist_grid");
                                }
                                (LibraryArtists, List) => {
                                    view_stack_clone.set_visible_child_name("artist_list");
                                }
                            }
                        }
                        SearchFilterChanged(_) => {
                            // Search filter changed - views should handle this internally
                            // through their own state observers
                            debug!("Search filter changed");
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

    scrolled_window.upcast::<Widget>()
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

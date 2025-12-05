//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{error::Error, sync::Arc};

use {
    libadwaita::{
        Application, ApplicationWindow, NavigationPage, NavigationView,
        glib::MainContext,
        gtk::{Box as GtkBox, Orientation::Vertical, ScrolledWindow, Widget},
        prelude::{
            AdwApplicationWindowExt, ApplicationExt, ApplicationExtManual, BoxExt, Cast,
            GtkWindowExt, ListModelExt, WidgetExt,
        },
    },
    parking_lot::RwLock,
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
        while let Ok(event) = receiver.recv().await {
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
    // Create main container with stack for view switching
    let main_container = GtkBox::builder().orientation(Vertical).spacing(12).build();

    // Wrap the main container in a scrolled window to provide vertical scrolling
    let scrolled_window = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&main_container)
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

    // Select initial view
    let initial_view = match (current_tab, current_view_mode) {
        (LibraryAlbums, Grid) => album_grid_view.widget.clone(),
        (LibraryAlbums, List) => album_list_view.widget.clone(),
        (LibraryArtists, Grid) => artist_grid_view.widget.clone(),
        (LibraryArtists, List) => artist_list_view.widget.clone(),
    };

    main_container.append(&initial_view.upcast::<Widget>());

    // Store the main container and views for later updates
    // We'll use a simple approach: replace the child when state changes
    let app_state_clone = app_state.clone();
    let main_container_clone = main_container.clone();
    let album_grid_widget = album_grid_view.widget.clone();
    let album_list_widget = album_list_view.widget.clone();
    let artist_grid_widget = artist_grid_view.widget.clone();
    let artist_list_widget = artist_list_view.widget.clone();

    // Subscribe to state changes for view updates
    MainContext::default().spawn_local(async move {
        let mut receiver = app_state_clone.subscribe();
        while let Ok(event) = receiver.recv().await {
            match event {
                LibraryStateChanged(new_state) => {
                    // Clear all children and add the new view
                    let children = main_container_clone.observe_children();
                    let n_items = children.n_items();
                    for i in 0..n_items {
                        if let Some(child) = children.item(i)
                            && let Some(widget) = child.downcast_ref::<Widget>()
                        {
                            main_container_clone.remove(widget);
                        }
                    }

                    // Determine new view based on state
                    let new_view = match (new_state.current_tab, new_state.view_mode) {
                        (LibraryAlbums, Grid) => album_grid_widget.clone().upcast::<Widget>(),
                        (LibraryAlbums, List) => album_list_widget.clone().upcast::<Widget>(),
                        (LibraryArtists, Grid) => artist_grid_widget.clone().upcast::<Widget>(),
                        (LibraryArtists, List) => artist_list_widget.clone().upcast::<Widget>(),
                    };

                    main_container_clone.append(&new_view);
                }
                SearchFilterChanged(_) => {
                    // Search filter changed - views should handle this internally
                    // through their own state observers
                }
                _ => {}
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

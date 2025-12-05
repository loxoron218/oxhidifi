//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{error::Error, sync::Arc};

use libadwaita::{
    Application, ApplicationWindow, NavigationPage, NavigationView,
    glib::MainContext,
    gtk::{
        Align::Start,
        Box as GtkBox, Button, Label,
        Orientation::{Horizontal, Vertical},
        Picture, Scale, ToggleButton, Widget,
    },
    prelude::{
        AdwApplicationWindowExt, ApplicationExt, ApplicationExtManual, BoxExt, Cast, GtkWindowExt,
        ListModelExt, RangeExt, WidgetExt,
    },
};

use crate::{
    audio::engine::AudioEngine,
    config::{SettingsManager, UserSettings},
    library::LibraryDatabase,
    state::{
        AppState,
        AppStateEvent::{LibraryStateChanged, SearchFilterChanged},
        ViewMode::{Grid, List},
        app_state::LibraryTab::{Albums as LibraryAlbums, Artists as LibraryArtists},
    },
    ui::{
        header_bar::HeaderBar,
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
    /// Application state manager.
    pub app_state: Arc<AppState>,
    /// User settings manager.
    pub settings: SettingsManager,
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
        let library_db = LibraryDatabase::new()
            .await
            .map_err(|e| format!("Failed to initialize library database: {}", e))?;

        // Create application state
        let app_state = AppState::new(Arc::downgrade(&Arc::new(audio_engine.clone())));

        let app = Application::builder()
            .application_id("com.example.oxhidifi")
            .build();

        Ok(OxhidifiApplication {
            app,
            audio_engine: Arc::new(audio_engine),
            library_db: Arc::new(library_db),
            app_state: Arc::new(app_state),
            settings,
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
            let settings_clone = self.settings.get_settings().clone();

            move |_| {
                build_ui(
                    &app_clone,
                    &audio_engine_clone,
                    &library_db_clone,
                    &app_state_clone,
                    &settings_clone,
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
    settings: &UserSettings,
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
    let main_content = create_main_content(app_state, settings, library_db, audio_engine);

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
    let player_bar = create_player_bar();

    // Assemble the main layout
    let main_box = GtkBox::builder().orientation(Vertical).build();

    main_box.append(&header_bar.widget);
    main_box.append(&navigation_view.upcast::<Widget>());
    main_box.append(&player_bar);

    // Set the window content
    window.set_content(Some(&main_box));
    window.present();
}

/// Creates the main content area with responsive layout.
fn create_main_content(
    app_state: &Arc<AppState>,
    settings: &UserSettings,
    _library_db: &Arc<LibraryDatabase>,
    _audio_engine: &Arc<AudioEngine>,
) -> Widget {
    // Create main container with stack for view switching
    let main_container = GtkBox::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let show_dr_badges = settings.show_dr_values;

    // Create all possible views upfront
    let album_grid_view = AlbumGridView::builder()
        .app_state(app_state.clone())
        .albums(Vec::new())
        .show_dr_badges(show_dr_badges)
        .compact(false)
        .build();

    let album_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Albums)
        .compact(false)
        .build();

    let artist_grid_view = ArtistGridView::builder()
        .app_state(app_state.clone())
        .artists(Vec::new())
        .compact(false)
        .build();

    let artist_list_view = ListView::builder()
        .app_state(app_state.clone())
        .view_type(Artists)
        .compact(false)
        .build();

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

    main_container.upcast::<Widget>()
}

/// Creates the persistent player control bar.
fn create_player_bar() -> GtkBox {
    let player_bar = GtkBox::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .css_classes(["player-bar"])
        .build();

    // Album artwork placeholder
    let artwork = Picture::builder()
        .width_request(48)
        .height_request(48)
        .build();
    player_bar.append(&artwork);

    // Track info placeholder
    let track_info = GtkBox::builder()
        .orientation(Vertical)
        .hexpand(true)
        .build();

    let title_label = Label::builder()
        .label("Track Title")
        .halign(Start)
        .xalign(0.0)
        .build();
    track_info.append(&title_label);

    let artist_label = Label::builder()
        .label("Artist Name")
        .halign(Start)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    track_info.append(&artist_label);

    player_bar.append(&track_info);

    // Player controls
    let controls = GtkBox::builder().orientation(Horizontal).spacing(6).build();

    let prev_button = Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .tooltip_text("Previous")
        .build();
    controls.append(&prev_button);

    let play_button = ToggleButton::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play")
        .build();
    controls.append(&play_button);

    let next_button = Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .tooltip_text("Next")
        .build();
    controls.append(&next_button);

    player_bar.append(&controls);

    // Volume control
    let volume_scale = Scale::builder()
        .orientation(Horizontal)
        .width_request(100)
        // Remove value() from builder, set it after creation
        .draw_value(false)
        .build();
    volume_scale.set_value(100.0);
    player_bar.append(&volume_scale);

    player_bar
}

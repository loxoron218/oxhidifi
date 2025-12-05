//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{error::Error, sync::Arc};

use libadwaita::{
    Application, ApplicationWindow, NavigationPage, NavigationView, TabView,
    gtk::{
        Align::Start,
        Box as GtkBox, Button, Label,
        Orientation::{Horizontal, Vertical},
        Picture, Scale, ToggleButton, Widget,
    },
    prelude::{
        AdwApplicationWindowExt, ApplicationExt, ApplicationExtManual, BoxExt, Cast, GtkWindowExt,
        RangeExt,
    },
};

use crate::{
    audio::engine::AudioEngine,
    config::{SettingsManager, UserSettings},
    library::LibraryDatabase,
    state::{
        AppState,
        ViewMode::List,
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
    // Create main container
    let main_container = GtkBox::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Create all possible views upfront
    let show_dr_badges = settings.show_dr_values;

    // Album views
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

    // Artist views
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

    // Create tab view for Albums/Artists navigation
    let tab_view = TabView::builder().build();

    // Initialize with current view mode from app state
    let current_view_mode = app_state.get_library_state().view_mode;

    let (album_view, artist_view) = if current_view_mode == List {
        (album_list_view.widget, artist_list_view.widget)
    } else {
        (album_grid_view.widget, artist_grid_view.widget)
    };

    tab_view.append(&album_view);
    tab_view.append(&artist_view);

    // Set tab titles
    tab_view.nth_page(0).set_title("Albums");
    tab_view.nth_page(1).set_title("Artists");

    // Connect tab view selection to app state
    let app_state_clone_tab = app_state.clone();
    tab_view.connect_selected_page_notify(move |tab_view| {
        if let Some(selected_page) = tab_view.selected_page() {
            let page_index = tab_view.page_position(&selected_page);
            let new_tab = if page_index == 0 {
                LibraryAlbums
            } else {
                LibraryArtists
            };

            // Update app state with new tab selection
            let mut library_state = app_state_clone_tab.get_library_state();
            library_state.current_tab = new_tab;
            app_state_clone_tab.update_library_state(library_state);
        }
    });

    main_container.append(&tab_view.upcast::<Widget>());

    // Connect to AppState for reactive view mode updates
    // Note: Full view recreation on mode change is complex and may cause issues
    // For now, we rely on the initial view mode setting
    // A more sophisticated implementation would recreate the tab view when needed

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

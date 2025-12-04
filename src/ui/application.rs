//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::sync::Arc;

use libadwaita::gtk::prelude::*;
use libadwaita::prelude::*;

use crate::{
    audio::engine::AudioEngine,
    config::{SettingsManager, UserSettings},
    library::LibraryDatabase,
    state::AppState,
};

/// Main application class with window management.
///
/// The `OxhidifiApplication` manages the main application window,
/// handles application lifecycle events, and coordinates between
/// different UI components.
pub struct OxhidifiApplication {
    /// The main application instance.
    pub app: libadwaita::Application,
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
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize settings
        let settings = SettingsManager::new()
            .map_err(|e| format!("Failed to initialize settings: {}", e))?;
        
        // Initialize audio engine
        let audio_engine = AudioEngine::new()
            .map_err(|e| format!("Failed to initialize audio engine: {}", e))?;
        
        // Initialize library database
        let library_db = LibraryDatabase::new()
            .await
            .map_err(|e| format!("Failed to initialize library database: {}", e))?;

        // Create application state
        let app_state = AppState::new(Arc::downgrade(&Arc::new(audio_engine.clone())));

        let app = libadwaita::Application::builder()
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
                build_ui(&app_clone, &audio_engine_clone, &library_db_clone, &app_state_clone, &settings_clone);
            }
        });

        self.app.run();
    }
}

/// Builds the main user interface.
fn build_ui(
    app: &libadwaita::Application,
    audio_engine: &Arc<AudioEngine>,
    library_db: &Arc<LibraryDatabase>,
    app_state: &Arc<AppState>,
    settings: &UserSettings,
) {
    // Create the main window
    let window = libadwaita::ApplicationWindow::builder()
        .application(app)
        .title("Oxhidifi")
        .default_width(1200)
        .default_height(800)
        .build();

    // Create header bar
    let header_bar = create_header_bar(settings);
    
    // Create main content area
    let main_content = libadwaita::gtk::Box::builder()
        .orientation(libadwaita::gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    
    // Add placeholder content
    let placeholder_label = libadwaita::gtk::Label::builder()
        .label("Welcome to Oxhidifi - High-Fidelity Music Player")
        .halign(libadwaita::gtk::Align::Center)
        .valign(libadwaita::gtk::Align::Center)
        .build();
    
    main_content.append(&placeholder_label);

    // Create player bar
    let player_bar = create_player_bar(app_state);

    // Assemble the main layout
    let main_box = libadwaita::gtk::Box::builder()
        .orientation(libadwaita::gtk::Orientation::Vertical)
        .build();
    
    main_box.append(&header_bar);
    main_box.append(&main_content);
    main_box.append(&player_bar);

    // Set the window content
    window.set_content(Some(&main_box));
    window.present();
}

/// Creates the application header bar.
fn create_header_bar(settings: &UserSettings) -> libadwaita::HeaderBar {
    let header_bar = libadwaita::HeaderBar::builder().build();

    // Search button (placeholder)
    let search_button = libadwaita::gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search")
        .build();
    header_bar.pack_start(&search_button);

    // View toggle button (placeholder)
    let view_toggle_icon = if settings.default_view_mode == "list" {
        "view-list-symbolic"
    } else {
        "view-grid-symbolic"
    };
    let view_toggle = libadwaita::gtk::ToggleButton::builder()
        .icon_name(view_toggle_icon)
        .tooltip_text("Toggle View")
        .build();
    header_bar.pack_start(&view_toggle);

    // Settings button (placeholder)
    let settings_button = libadwaita::gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Settings")
        .build();
    header_bar.pack_end(&settings_button);

    // Tab navigation (placeholder)
    let tab_view = libadwaita::TabView::builder().build();
    
    // TabPage doesn't have a new() constructor, create pages differently
    let albums_page = libadwaita::gtk::Label::new(Some("Albums"));
    tab_view.append(&albums_page.upcast::<libadwaita::gtk::Widget>());
    
    let artists_page = libadwaita::gtk::Label::new(Some("Artists"));
    tab_view.append(&artists_page.upcast::<libadwaita::gtk::Widget>());
    
    header_bar.set_title_widget(Some(&tab_view));

    header_bar
}

/// Creates the persistent player control bar.
fn create_player_bar(_app_state: &Arc<AppState>) -> libadwaita::gtk::Box {
    let player_bar = libadwaita::gtk::Box::builder()
        .orientation(libadwaita::gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .css_classes(vec!["player-bar".to_string()])
        .build();

    // Album artwork placeholder
    let artwork = libadwaita::gtk::Picture::builder()
        .width_request(48)
        .height_request(48)
        .build();
    player_bar.append(&artwork);

    // Track info placeholder
    let track_info = libadwaita::gtk::Box::builder()
        .orientation(libadwaita::gtk::Orientation::Vertical)
        .hexpand(true)
        .build();
    
    let title_label = libadwaita::gtk::Label::builder()
        .label("Track Title")
        .halign(libadwaita::gtk::Align::Start)
        .xalign(0.0)
        .build();
    track_info.append(&title_label);
    
    let artist_label = libadwaita::gtk::Label::builder()
        .label("Artist Name")
        .halign(libadwaita::gtk::Align::Start)
        .xalign(0.0)
        .css_classes(vec!["dim-label".to_string()])
        .build();
    track_info.append(&artist_label);
    
    player_bar.append(&track_info);

    // Player controls
    let controls = libadwaita::gtk::Box::builder()
        .orientation(libadwaita::gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    
    let prev_button = libadwaita::gtk::Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .tooltip_text("Previous")
        .build();
    controls.append(&prev_button);
    
    let play_button = libadwaita::gtk::ToggleButton::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play")
        .build();
    controls.append(&play_button);
    
    let next_button = libadwaita::gtk::Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .tooltip_text("Next")
        .build();
    controls.append(&next_button);
    
    player_bar.append(&controls);

    // Volume control
    let volume_scale = libadwaita::gtk::Scale::builder()
        .orientation(libadwaita::gtk::Orientation::Horizontal)
        .width_request(100)
        // Remove value() from builder, set it after creation
        .draw_value(false)
        .build();
    volume_scale.set_value(100.0);
    player_bar.append(&volume_scale);

    player_bar
}
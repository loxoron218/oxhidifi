//! Main application window and navigation structure.
//!
//! This module implements the `OxhidifiApplication` which serves as the
//! main entry point for the Libadwaita-based user interface.

use std::{error::Error, sync::Arc};

use libadwaita::{
    Application, ApplicationWindow, HeaderBar, TabView,
    gtk::{
        Align::{Center, Start},
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
    state::AppState,
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
    _audio_engine: &Arc<AudioEngine>,
    _library_db: &Arc<LibraryDatabase>,
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

    // Create header bar
    let header_bar = create_header_bar(settings);

    // Create main content area
    let main_content = GtkBox::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Add placeholder content
    let placeholder_label = Label::builder()
        .label("Welcome to Oxhidifi - High-Fidelity Music Player")
        .halign(Center)
        .valign(Center)
        .build();

    main_content.append(&placeholder_label);

    // Create player bar
    let player_bar = create_player_bar(app_state);

    // Assemble the main layout
    let main_box = GtkBox::builder().orientation(Vertical).build();

    main_box.append(&header_bar);
    main_box.append(&main_content);
    main_box.append(&player_bar);

    // Set the window content
    window.set_content(Some(&main_box));
    window.present();
}

/// Creates the application header bar.
fn create_header_bar(settings: &UserSettings) -> HeaderBar {
    let header_bar = HeaderBar::builder().build();

    // Search button (placeholder)
    let search_button = ToggleButton::builder()
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
    let view_toggle = ToggleButton::builder()
        .icon_name(view_toggle_icon)
        .tooltip_text("Toggle View")
        .build();
    header_bar.pack_start(&view_toggle);

    // Settings button (placeholder)
    let settings_button = Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Settings")
        .build();
    header_bar.pack_end(&settings_button);

    // Tab navigation (placeholder)
    let tab_view = TabView::builder().build();

    // TabPage doesn't have a new() constructor, create pages differently
    let albums_page = Label::new(Some("Albums"));
    tab_view.append(&albums_page.upcast::<Widget>());

    let artists_page = Label::new(Some("Artists"));
    tab_view.append(&artists_page.upcast::<Widget>());

    header_bar.set_title_widget(Some(&tab_view));

    header_bar
}

/// Creates the persistent player control bar.
fn create_player_bar(_app_state: &Arc<AppState>) -> GtkBox {
    let player_bar = GtkBox::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .css_classes(vec!["player-bar".to_string()])
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
        .css_classes(vec!["dim-label".to_string()])
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

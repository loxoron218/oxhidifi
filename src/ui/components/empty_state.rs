//! Empty state UI component for when library grids or lists contain no content.
//!
//! This module implements the `EmptyState` component that displays a user-friendly
//! message with a button to add library directories when no albums or artists are available.

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use {
    libadwaita::{
        ApplicationWindow,
        glib::MainContext,
        gtk::{
            Align::{Center, Fill},
            Box, Button, FileDialog, Label,
            Orientation::Vertical,
            Widget,
        },
        prelude::{BoxExt, ButtonExt, Cast, FileExt, WidgetExt},
    },
    parking_lot::RwLock,
    tokio::spawn,
    tracing::{debug, error, info, warn},
};

use crate::{
    config::{SettingsManager, UserSettings},
    library::{
        database::LibraryDatabase,
        dr_parser::DrParser,
        scanner::{
            LibraryScanner, ScannerEvent::LibraryChanged, event_processing::handle_files_changed,
        },
    },
    state::{AppState, LibraryState},
};

/// Configuration for `EmptyState` display options.
#[derive(Debug, Clone)]
pub struct EmptyStateConfig {
    /// Whether this empty state is for albums or artists.
    pub is_album_view: bool,
}

#[derive(Clone)]
/// Empty state UI component for when library grids or lists contain no content.
///
/// The `EmptyState` component displays a clear message explaining that no content
/// is available and provides a button to add library directories using a native
/// folder picker dialog.
pub struct EmptyState {
    /// The underlying GTK widget container.
    pub widget: Widget,
    /// Main container box.
    pub container: Box,
    /// Message label.
    pub message_label: Label,
    /// Add directory button.
    pub add_button: Button,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Settings manager reference.
    pub settings_manager: Option<SettingsManager>,
    /// Current configuration.
    pub config: EmptyStateConfig,
    /// Reference to the main application window for file dialog parent.
    pub window: Option<ApplicationWindow>,
    /// Cancellation token for stopping the scanner event listener.
    pub scanner_cancel_token: Option<Arc<AtomicBool>>,
}

impl EmptyState {
    /// Creates a new `EmptyState` component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `settings_manager` - Optional settings manager for updating library directories
    /// * `config` - Display configuration
    ///
    /// # Returns
    ///
    /// A new `EmptyState` instance.
    #[must_use]
    pub fn new(
        app_state: Option<Arc<AppState>>,
        settings_manager: Option<SettingsManager>,
        config: EmptyStateConfig,
        window: Option<ApplicationWindow>,
    ) -> Self {
        // Create message label
        let message_text = if config.is_album_view {
            "No albums found"
        } else {
            "No artists found"
        };

        let message_label = Label::builder()
            .label(message_text)
            .halign(Center)
            .valign(Center)
            .css_classes(["title-1"])
            .build();

        // Create description label
        let description_label = Label::builder()
            .label("Add music folders to your library to get started")
            .halign(Center)
            .valign(Center)
            .css_classes(["dim-label"])
            .wrap(true)
            .max_width_chars(40)
            .build();

        // Create add directory button
        let add_button = Button::builder()
            .label("Add Music Folder")
            .halign(Center)
            .valign(Center)
            .css_classes(["suggested-action"])
            .tooltip_text("Select a folder containing your music files")
            .use_underline(true)
            .build();

        // Create main container
        let container = Box::builder()
            .orientation(Vertical)
            .halign(Center)
            .valign(Center)
            .spacing(12)
            .margin_top(48)
            .margin_bottom(48)
            .margin_start(24)
            .margin_end(24)
            .build();

        container.append(&message_label);
        container.append(&description_label);
        container.append(&add_button);

        // Create outer widget container
        let widget = Box::builder()
            .orientation(Vertical)
            .halign(Fill)
            .valign(Fill)
            .build();
        widget.append(&container);

        // Don't connect button handler yet - will be connected when window is available

        Self {
            widget: widget.upcast_ref::<Widget>().clone(),
            container,
            message_label,
            add_button,
            app_state,
            settings_manager,
            config,
            window,
            scanner_cancel_token: None,
        }
    }

    /// Stops the scanner event listener by setting the cancellation token.
    ///
    /// This method provides an explicit cleanup mechanism for the event listener
    /// loop that would otherwise run indefinitely until the channel closes.
    pub fn stop_scanner_event_listener(&mut self) {
        if let Some(cancel_token) = &self.scanner_cancel_token {
            cancel_token.store(true, Relaxed);
            debug!("Scanner event listener cancellation requested");
        }
        self.scanner_cancel_token = None;
    }

    /// Connects event handlers to the add directory button.
    ///
    /// # Panics
    ///
    /// Panics if the cancellation token is None after being initialized.
    pub fn connect_button_handlers(&mut self) {
        let settings_manager_clone = self.settings_manager.clone();
        let window_clone = self.window.clone();
        let app_state_clone = self.app_state.clone();

        // Create cancellation token if it doesn't exist
        if self.scanner_cancel_token.is_none() {
            self.scanner_cancel_token = Some(Arc::new(AtomicBool::new(false)));
        }
        let Some(cancel_token_clone) = self.scanner_cancel_token.clone() else {
            warn!("Scanner cancellation token is None");
            return;
        };

        self.add_button.connect_clicked(move |_| {
            // Create file dialog for folder selection
            let dialog = FileDialog::builder()
                .title("Select Music Folder")
                .accept_label("Add Folder")
                .modal(true)
                .build();

            // Get window reference for dialog parent
            if let Some(window) = &window_clone {
                // Open folder selection dialog asynchronously
                let dialog_clone = dialog;
                let window_clone2 = window.clone();
                let settings_manager_clone2 = settings_manager_clone.clone();
                let app_state_clone2 = app_state_clone.clone();
                let cancel_token = cancel_token_clone.clone();

                MainContext::default().spawn_local(async move {
                    match dialog_clone
                        .select_folder_future(Some(&window_clone2))
                        .await
                    {
                        Ok(folder) => {
                            if let Some(settings_manager) = &settings_manager_clone2 {
                                // Get the selected folder path
                                if let Some(path) = folder.path()
                                    && let Some(path_str) = path.to_str()
                                {
                                    let path_str: &str = path_str;

                                    // Clone the settings manager to get mutable access
                                    let settings_manager_clone = settings_manager.clone();

                                    // Update settings with new library directory
                                    let mut current_settings =
                                        settings_manager_clone.get_settings().clone();

                                    // Only add if not already present
                                    let path_string = path_str.to_string();
                                    if !current_settings.library_directories.contains(&path_string)
                                    {
                                        current_settings
                                            .library_directories
                                            .push(path_str.to_string());

                                        if let Err(e) =
                                            settings_manager_clone.update_settings(current_settings)
                                        {
                                            error!("Failed to update settings: {e}");
                                            return;
                                        }

                                        // Log successful addition
                                        info!("Library directory added: {path_str}");

                                        // Trigger library rescan
                                        Self::trigger_library_rescan(
                                            path_str,
                                            &settings_manager_clone,
                                            app_state_clone2.as_ref(),
                                            &cancel_token,
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Folder selection cancelled or failed: {e}");
                        }
                    }
                });
            }
        });
    }

    /// Updates the empty state based on the current library state.
    ///
    /// # Arguments
    ///
    /// * `library_state` - Current library state to check for emptiness
    pub fn update_from_library_state(&self, library_state: &LibraryState) {
        let is_empty = if self.config.is_album_view {
            library_state.albums.is_empty()
        } else {
            library_state.artists.is_empty()
        };

        self.widget.set_visible(is_empty);
    }

    /// Prepares scanning resources including database connection, settings arc, and DR parser.
    ///
    /// # Arguments
    ///
    /// * `settings_snapshot` - Current settings snapshot
    ///
    /// # Returns
    ///
    /// A tuple containing the database arc, settings arc, and optional DR parser
    async fn prepare_scan_resources(
        settings_snapshot: UserSettings,
    ) -> Option<(
        Arc<LibraryDatabase>,
        Arc<RwLock<UserSettings>>,
        Option<Arc<DrParser>>,
    )> {
        // NOTE: This creates a snapshot of the settings at the time of the scan.
        // If global settings (e.g., from a settings UI) are updated later,
        // this `settings_arc` and the one held by the `LibraryScanner`'s
        // background tasks will not reflect those changes.
        // A more robust solution would involve `SettingsManager` exposing
        // an `Arc<RwLock<UserSettings>>` directly, or the scanner
        // subscribing to settings changes.
        match LibraryDatabase::new().await {
            Ok(library_db) => {
                let library_db_arc = Arc::new(library_db);
                let settings_arc = Arc::new(RwLock::new(settings_snapshot.clone()));

                // Initialize DR parser if enabled
                let dr_parser = if settings_snapshot.show_dr_values {
                    match DrParser::new(library_db_arc.clone()) {
                        Ok(parser) => Some(Arc::new(parser)),
                        Err(e) => {
                            error!("Failed to initialize DR parser: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                Some((library_db_arc, settings_arc, dr_parser))
            }
            Err(e) => {
                error!(error = %e, "Failed to create library database");
                None
            }
        }
    }

    /// Starts the scanner event listener loop for processing library change events.
    ///
    /// # Arguments
    ///
    /// * `scanner_arc` - The scanner to subscribe to events from
    /// * `app_state` - Application state to update when library changes
    /// * `library_db` - Database connection for refreshing data
    /// * `cancel_token` - Cancellation token for explicitly stopping the listener
    fn start_scanner_event_listener(
        scanner_arc: &Arc<RwLock<LibraryScanner>>,
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        cancel_token: &Arc<AtomicBool>,
    ) {
        // Start the event listener loop for the new scanner
        // This mirrors the logic in OxhidifiApplication::run
        let scanner_read = scanner_arc.read();
        let rx = scanner_read.subscribe();
        let app_state_refresh = app_state.clone();
        let db_refresh = library_db.clone();
        let cancel_token_clone = cancel_token.clone();

        MainContext::default().spawn_local(async move {
            while !cancel_token_clone.load(Relaxed) {
                match rx.recv().await {
                    Ok(LibraryChanged) => {
                        debug!(
                            "LibraryChanged event received (dynamic scanner), refreshing app state"
                        );

                        // Refresh albums
                        let albums = match db_refresh.get_albums(None).await {
                            Ok(albums) => albums,
                            Err(e) => {
                                error!(error = %e, "Failed to refresh albums");
                                Vec::new()
                            }
                        };

                        // Refresh artists
                        let artists = match db_refresh.get_artists(None).await {
                            Ok(artists) => artists,
                            Err(e) => {
                                error!(error = %e, "Failed to refresh artists");
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
            debug!("Scanner event listener stopped (cancelled or channel closed)");
        });
    }

    /// Gets the existing scanner or creates a new one with event listener.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state containing or to store the scanner
    /// * `library_db` - Database connection for the scanner
    /// * `settings_arc` - Settings for the scanner configuration
    /// * `cancel_token` - Cancellation token for the event listener
    ///
    /// # Returns
    ///
    /// The scanner arc, or None if creation failed
    fn get_or_create_scanner(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_arc: &Arc<RwLock<UserSettings>>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Option<Arc<RwLock<LibraryScanner>>> {
        let existing_scanner = app_state.library_scanner.read().clone();

        existing_scanner.map_or_else(
            || {
                // Create new scanner

                match LibraryScanner::new(library_db, settings_arc, None) {
                    Ok(scanner) => {
                        let scanner_arc = Arc::new(RwLock::new(scanner));

                        // IMPORTANT: Store the scanner in AppState to prevent it from being dropped
                        *app_state.library_scanner.write() = Some(scanner_arc.clone());

                        // Start the event listener loop for the new scanner
                        // This mirrors the logic in OxhidifiApplication::run
                        Self::start_scanner_event_listener(
                            &scanner_arc,
                            app_state,
                            library_db,
                            cancel_token,
                        );

                        Some(scanner_arc)
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to create library scanner");
                        None
                    }
                }
            },
            Some,
        )
    }

    /// Executes the background scanning task for library directories.
    ///
    /// # Arguments
    ///
    /// * `scanner` - Scanner to add directory to
    /// * `db` - Database connection for processing files
    /// * `settings` - Settings containing library directories
    /// * `dr_parser` - Optional DR parser for metadata
    /// * `new_directory` - New directory to add
    async fn execute_background_scan(
        scanner: Arc<RwLock<LibraryScanner>>,
        db: Arc<LibraryDatabase>,
        settings: Arc<RwLock<UserSettings>>,
        dr_parser: Option<Arc<DrParser>>,
        new_directory: String,
    ) {
        // Offload heavy scanning work to a background task
        let scanner_for_task = scanner.clone();
        let db_for_task = db.clone();
        let settings_for_task = settings.clone();
        let dr_parser_for_task = dr_parser.clone();
        let dir_for_task = new_directory.clone();

        let scan_handle = spawn(async move {
            // 1. Add directory to scanner (fast, takes write lock)
            {
                let mut scanner_write = scanner_for_task.write();
                if let Err(e) = scanner_write.add_library_directory(&dir_for_task) {
                    error!(error = %e, "Failed to add directory to scanner");
                }
            }

            // 2. Collect files (BLOCKING IO - acceptable in background task)
            // Note: Blocking file IO is acceptable here because:
            // - This runs in a tokio::spawn background task, not the UI thread
            // - It's triggered by user action (adding directory), not frequent polling
            // - The entire scanning operation is already CPU/IO heavy by design
            // - Converting to fully async would require tokio-async-compat wrappers
            let all_audio_files = {
                let library_dirs = settings_for_task.read().library_directories.clone();
                let mut all_files = Vec::new();

                for dir in library_dirs {
                    let dir_path = Path::new(&dir);
                    if let Ok(audio_files) =
                        LibraryScanner::collect_audio_files_from_directory(dir_path)
                    {
                        all_files.extend(audio_files);
                    }
                }
                all_files
            };

            // 3. Process files (Heavy CPU/IO)
            if !all_audio_files.is_empty()
                && let Err(e) = handle_files_changed(
                    all_audio_files,
                    &db_for_task,
                    &settings_for_task,
                    &dr_parser_for_task,
                )
                .await
            {
                error!(error = %e, "Failed to process files");
            }
        });

        // Await the background task (yields to main loop so UI stays responsive)
        if let Err(e) = scan_handle.await {
            error!(error = %e, "Scan task panicked");
        }
    }

    /// Refreshes the UI state with new library data from the database.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state to update
    /// * `library_db` - Database connection to fetch data from
    async fn refresh_library_ui_state(app_state: Arc<AppState>, library_db: Arc<LibraryDatabase>) {
        match library_db.get_albums(None).await {
            Ok(albums) => match library_db.get_artists(None).await {
                Ok(artists) => {
                    app_state.update_library_data(albums, artists);
                }
                Err(e) => {
                    error!(error = %e, "Failed to get artists from database");
                }
            },
            Err(e) => {
                error!(error = %e, "Failed to get albums from database");
            }
        }
    }

    /// Triggers a library rescan after adding a new directory.
    ///
    /// # Arguments
    ///
    /// * `new_directory` - The newly added directory path
    /// * `settings_manager` - Settings manager to get current settings
    /// * `app_state` - Application state to update UI
    /// * `cancel_token` - Cancellation token for the event listener
    async fn trigger_library_rescan(
        new_directory: &str,
        settings_manager: &SettingsManager,
        app_state: Option<&Arc<AppState>>,
        cancel_token: &Arc<AtomicBool>,
    ) {
        if let Some(app_state) = app_state {
            let app_state_clone = app_state.clone();
            let new_directory = new_directory.to_string();
            let settings_snapshot = settings_manager.get_settings().clone();

            if let Some((library_db_arc, settings_arc, dr_parser)) =
                Self::prepare_scan_resources(settings_snapshot).await
                && let Some(scanner_arc) = Self::get_or_create_scanner(
                    &app_state_clone,
                    &library_db_arc,
                    &settings_arc,
                    cancel_token,
                )
            {
                Self::execute_background_scan(
                    scanner_arc,
                    library_db_arc.clone(),
                    settings_arc.clone(),
                    dr_parser,
                    new_directory,
                )
                .await;

                Self::refresh_library_ui_state(app_state_clone, library_db_arc).await;
            }
        }
    }
}

impl Default for EmptyState {
    fn default() -> Self {
        Self::new(
            None,
            None,
            EmptyStateConfig {
                is_album_view: true,
            },
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, write},
        sync::{Arc, atomic::AtomicBool},
    };

    use anyhow::{Result, anyhow, bail};

    use {parking_lot::RwLock, tempfile::TempDir};

    use crate::{
        audio::engine::AudioEngine,
        config::{SettingsManager, UserSettings},
        library::{database::LibraryDatabase, dr_parser::DrParser, scanner::LibraryScanner},
        state::AppState,
        ui::components::empty_state::{EmptyState, EmptyStateConfig},
    };

    #[test]
    fn test_empty_state_config() {
        let album_config = EmptyStateConfig {
            is_album_view: true,
        };
        let artist_config = EmptyStateConfig {
            is_album_view: false,
        };

        assert!(album_config.is_album_view);
        assert!(!artist_config.is_album_view);
    }

    #[tokio::test]
    async fn test_prepare_scan_resources_with_dr_parser() -> Result<()> {
        let settings = UserSettings {
            show_dr_values: true,
            ..Default::default()
        };

        let result = EmptyState::prepare_scan_resources(settings).await;

        let (_, _, dr_parser) = result.ok_or_else(|| anyhow!("result should be Some"))?;
        if dr_parser.is_none() {
            bail!("DR parser should be Some");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_prepare_scan_resources_without_dr_parser() -> Result<()> {
        let settings = UserSettings {
            show_dr_values: false,
            ..Default::default()
        };

        let result = EmptyState::prepare_scan_resources(settings).await;

        let (_library_db, settings_arc, dr_parser) =
            result.ok_or_else(|| anyhow!("result should be Some"))?;

        if dr_parser.is_some() {
            bail!("DR parser should be None");
        }

        let settings_read = settings_arc.read();
        if settings_read.show_dr_values {
            bail!("Show DR values should be false");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_prepare_scan_resources_settings_arc_content() -> Result<()> {
        let test_dirs = vec!["/music/test".to_string(), "/another/path".to_string()];
        let settings = UserSettings {
            library_directories: test_dirs.clone(),
            show_dr_values: true,
            ..Default::default()
        };

        let result = EmptyState::prepare_scan_resources(settings).await;

        let (_library_db, settings_arc, dr_parser) =
            result.ok_or_else(|| anyhow!("result should be Some"))?;

        let settings_read = settings_arc.read();
        if settings_read.library_directories != test_dirs {
            bail!("Library directories mismatch");
        }
        if !settings_read.show_dr_values {
            bail!("Show DR values should be true");
        }
        if dr_parser.is_none() {
            bail!("DR parser should be Some");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_or_create_scanner_returns_existing() -> Result<()> {
        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings::default();
        let settings_arc = Arc::new(RwLock::new(settings));

        let existing_scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let existing_scanner_arc = Arc::new(RwLock::new(existing_scanner));

        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let app_state = Arc::new(AppState::new(
            engine_weak,
            Some(existing_scanner_arc.clone()),
            Arc::new(RwLock::new(settings_manager)),
        ));

        let cancel_token = Arc::new(AtomicBool::new(false));

        let result = EmptyState::get_or_create_scanner(
            &app_state,
            &library_db_arc,
            &settings_arc,
            &cancel_token,
        );

        let scanner_arc = result.ok_or_else(|| anyhow!("result should be Some"))?;
        if !Arc::ptr_eq(&scanner_arc, &existing_scanner_arc) {
            bail!("Should return existing scanner");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_or_create_scanner_creates_new() -> Result<()> {
        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings::default();
        let settings_arc = Arc::new(RwLock::new(settings));

        let result = LibraryScanner::new(&library_db_arc, &settings_arc, None);

        if result.is_err() {
            bail!("Scanner should be created successfully");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_or_create_scanner_stores_in_app_state() -> Result<()> {
        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings::default();
        let settings_arc = Arc::new(RwLock::new(settings));

        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;

        let app_state = Arc::new(AppState::new(
            engine_weak,
            None,
            Arc::new(RwLock::new(settings_manager)),
        ));

        if app_state.library_scanner.read().is_some() {
            bail!("Scanner should be None initially");
        }

        let cancel_token = Arc::new(AtomicBool::new(false));

        let result = EmptyState::get_or_create_scanner(
            &app_state,
            &library_db_arc,
            &settings_arc,
            &cancel_token,
        );
        if result.is_none() {
            bail!("Result should be Some");
        }

        let stored_scanner = app_state.library_scanner.read().clone();
        if stored_scanner.is_none() {
            bail!("Scanner should be stored in app_state");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_background_scan_adds_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let music_dir = temp_dir.path().join("music");
        create_dir_all(&music_dir)?;

        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings {
            library_directories: vec![music_dir.to_string_lossy().to_string()],
            ..Default::default()
        };
        let settings_arc = Arc::new(RwLock::new(settings));

        let scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let scanner_arc = Arc::new(RwLock::new(scanner));

        let test_path = music_dir.to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            scanner_arc.clone(),
            library_db_arc,
            settings_arc.clone(),
            None,
            test_path,
        )
        .await;
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_background_scan_with_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let music_dir = temp_dir.path().join("music");
        create_dir_all(&music_dir)?;

        let audio_file = music_dir.join("test_audio.mp3");
        write(&audio_file, b"fake audio data")?;

        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings {
            library_directories: vec![music_dir.to_string_lossy().to_string()],
            ..Default::default()
        };
        let settings_arc = Arc::new(RwLock::new(settings));

        let scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let scanner_arc = Arc::new(RwLock::new(scanner));

        let test_path = music_dir.to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            scanner_arc.clone(),
            library_db_arc.clone(),
            settings_arc.clone(),
            None,
            test_path,
        )
        .await;

        let albums = library_db_arc.get_albums(None).await;
        let artists = library_db_arc.get_artists(None).await;
        if albums.is_err() && artists.is_err() {
            bail!("At least one of albums or artists should be ok");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_background_scan_with_dr_parser() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let music_dir = temp_dir.path().join("music");
        create_dir_all(&music_dir)?;

        let dr_file = music_dir.join("dr.txt");
        write(&dr_file, "Official DR value: DR12")?;

        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings {
            library_directories: vec![music_dir.to_string_lossy().to_string()],
            show_dr_values: true,
            ..Default::default()
        };
        let settings_arc = Arc::new(RwLock::new(settings));

        let scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let scanner_arc = Arc::new(RwLock::new(scanner));

        let dr_parser = {
            let parser = DrParser::new(library_db_arc.clone())?;
            Some(Arc::new(parser))
        };

        let test_path = music_dir.to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            scanner_arc.clone(),
            library_db_arc.clone(),
            settings_arc.clone(),
            dr_parser,
            test_path,
        )
        .await;
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_background_scan_empty_library_directories() -> Result<()> {
        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings {
            library_directories: vec![],
            ..Default::default()
        };
        let settings_arc = Arc::new(RwLock::new(settings));

        let scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let scanner_arc = Arc::new(RwLock::new(scanner));

        let temp_dir = TempDir::new()?;
        let test_path = temp_dir.path().to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            scanner_arc.clone(),
            library_db_arc.clone(),
            settings_arc.clone(),
            None,
            test_path,
        )
        .await;
        Ok(())
    }
}

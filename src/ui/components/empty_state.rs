//! Empty state UI component for when library grids or lists contain no content.
//!
//! This module implements the `EmptyState` component that displays a user-friendly
//! message with a button to add library directories when no albums or artists are available.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
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
            ProgressBar, Spinner, Widget,
        },
        prelude::{BoxExt, ButtonExt, Cast, FileExt, WidgetExt},
    },
    num_traits::ToPrimitive,
    parking_lot::RwLock,
    tokio::{select, spawn, sync::RwLock as TokioRwLock, try_join},
    tracing::{debug, error, info, warn},
};

use crate::{
    config::settings::{SettingsManager, UserSettings},
    library::{
        database::LibraryDatabase,
        dr_parser::DrParser,
        scanner::{
            LibraryScanner, ScannerEvent::LibraryChanged, event_processing::handle_files_changed,
        },
    },
    state::app_state::{
        AppState,
        AppStateEvent::{LibraryDataChanged, LibraryScanProgress, LibraryScanningChanged},
        LibraryState,
    },
};

/// Progress update interval for library scanning (in number of albums processed)
pub const SCAN_PROGRESS_INTERVAL: usize = 20;

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
    /// Description label.
    pub description_label: Label,
    /// Add directory button.
    pub add_button: Button,
    /// Progress spinner (shown during file discovery).
    pub progress_spinner: Option<Spinner>,
    /// Progress bar (shown during processing).
    pub progress_bar: Option<ProgressBar>,
    /// Progress status label.
    pub progress_label: Option<Label>,
    /// Cancel scan button.
    pub cancel_button: Option<Button>,
    /// Whether a scan is currently in progress.
    pub is_scanning: Arc<AtomicBool>,
    /// Application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Settings manager reference.
    pub settings_manager: Option<Arc<RwLock<SettingsManager>>>,
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
        settings_manager: Option<Arc<RwLock<SettingsManager>>>,
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

        // Create progress spinner (hidden initially)
        let progress_spinner = Spinner::builder()
            .spinning(true)
            .halign(Center)
            .visible(false)
            .build();
        progress_spinner.set_size_request(48, 48);

        // Create progress bar (hidden initially)
        let progress_bar = ProgressBar::builder()
            .halign(Center)
            .hexpand(true)
            .visible(false)
            .build();
        progress_bar.set_fraction(0.0);

        // Create progress label (hidden initially)
        let progress_label = Label::builder()
            .label("Scanning...")
            .halign(Center)
            .css_classes(["dim-label"])
            .visible(false)
            .wrap(true)
            .max_width_chars(40)
            .build();

        // Create cancel button (hidden initially)
        let cancel_button = Button::builder()
            .label("Cancel")
            .halign(Center)
            .css_classes(["destructive-action"])
            .tooltip_text("Cancel the current scan")
            .visible(false)
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
        container.append(&progress_spinner);
        container.append(&progress_bar);
        container.append(&progress_label);
        container.append(&cancel_button);

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
            description_label,
            add_button,
            progress_spinner: Some(progress_spinner),
            progress_bar: Some(progress_bar),
            progress_label: Some(progress_label),
            cancel_button: Some(cancel_button),
            is_scanning: Arc::new(AtomicBool::new(false)),
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
}

impl Drop for EmptyState {
    fn drop(&mut self) {
        self.stop_scanner_event_listener();
    }
}

impl EmptyState {
    /// Connects event handlers to the add directory button.
    ///
    /// # Panics
    ///
    /// Panics if the cancellation token is None after being initialized.
    pub fn connect_button_handlers(&mut self) {
        let settings_manager_clone = self.settings_manager.as_ref().map(Arc::clone);
        let window_clone = self.window.clone();
        let app_state_clone = self.app_state.as_ref().map(Arc::clone);

        // Create cancellation token if it doesn't exist
        if self.scanner_cancel_token.is_none() {
            self.scanner_cancel_token = Some(Arc::new(AtomicBool::new(false)));
        }
        let Some(cancel_token_clone) = self.scanner_cancel_token.as_ref().map(Arc::clone) else {
            warn!(
                token_type = "scanner_cancellation",
                "Scanner cancellation token is None"
            );
            return;
        };

        let add_button_clone = self.add_button.clone();

        self.add_button.connect_clicked(move |_| {
            let Some(settings_manager) = settings_manager_clone.clone() else {
                return;
            };

            // Create file dialog for folder selection
            let dialog = FileDialog::builder()
                .title("Select Music Folder")
                .accept_label("Add Folder")
                .modal(true)
                .build();

            // Get window reference for dialog parent
            if let Some(window) = &window_clone {
                Self::handle_folder_selection(
                    dialog,
                    window.clone(),
                    settings_manager,
                    app_state_clone.clone(),
                    Arc::clone(&cancel_token_clone),
                    &add_button_clone,
                );
            }
        });

        if let Some(cancel_button) = &self.cancel_button {
            let cancel_token_clone = self.scanner_cancel_token.as_ref().map(Arc::clone);
            let app_state_clone = self.app_state.as_ref().map(Arc::clone);
            cancel_button.connect_clicked(move |button| {
                if let Some(token) = &cancel_token_clone {
                    token.store(true, Relaxed);
                    debug!("Scan cancellation requested via button");
                }
                if let Some(app) = &app_state_clone {
                    app.set_scanning(false);
                }
                button.set_visible(false);
            });
        }
    }

    /// Handles folder selection and triggers library rescan.
    ///
    /// # Arguments
    ///
    /// * `dialog` - File dialog for folder selection
    /// * `window` - Parent window for the dialog
    /// * `settings_manager` - Settings manager for adding library directory
    /// * `app_state` - Application state for triggering rescan
    /// * `cancel_token` - Cancellation token for stopping scan
    /// * `add_button` - Add button to hide after selection
    fn handle_folder_selection(
        dialog: FileDialog,
        window: ApplicationWindow,
        settings_manager: Arc<RwLock<SettingsManager>>,
        app_state: Option<Arc<AppState>>,
        cancel_token: Arc<AtomicBool>,
        add_button: &Button,
    ) {
        add_button.set_visible(false);

        MainContext::default().spawn_local(async move {
            match dialog.select_folder_future(Some(&window)).await {
                Ok(folder) => {
                    // Get the selected folder path
                    let Some(path) = folder.path() else {
                        return;
                    };

                    let Some(path_str) = path.to_str() else {
                        return;
                    };

                    // Only add if not already present
                    let path_string = path_str.to_string();

                    // Get mutable reference to update settings in a tight scope
                    let add_result = {
                        let settings_write = settings_manager.write();
                        settings_write.add_library_directory(&path_string)
                    };

                    if let Err(e) = add_result {
                        error!(error = %e, "Failed to add library directory");
                        return;
                    }

                    // Log successful addition
                    info!("Library directory added: {path_string}");

                    cancel_token.store(false, Relaxed);
                    Self::trigger_library_rescan(
                        path_str,
                        settings_manager,
                        app_state,
                        cancel_token,
                    )
                    .await;
                }
                Err(e) => {
                    warn!(error = %e, "Folder selection cancelled or failed");
                }
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

        // Reset progress widgets when library is empty (e.g., after folder removal)
        if is_empty {
            if let Some(spinner) = &self.progress_spinner {
                spinner.set_visible(false);
            }
            if let Some(progress_bar) = &self.progress_bar {
                progress_bar.set_visible(false);
            }
            if let Some(progress_label) = &self.progress_label {
                progress_label.set_visible(false);
            }
            if let Some(cancel_button) = &self.cancel_button {
                cancel_button.set_visible(false);
            }
            self.is_scanning.store(false, Relaxed);

            // Show the regular empty state UI
            self.add_button.set_visible(true);
            self.message_label.set_visible(true);
            self.description_label.set_visible(true);
        }
    }

    /// Starts listening to `AppState` events for scanning state changes.
    ///
    /// This allows the `EmptyState` to show progress UI when scanning is triggered
    /// from other parts of the application (e.g., Library preferences page).
    pub fn start_scanning_subscription(&mut self) {
        let Some(app_state) = &self.app_state else {
            return;
        };

        let Some(spinner) = self.progress_spinner.as_ref() else {
            return;
        };

        if self.scanner_cancel_token.is_none() {
            self.scanner_cancel_token = Some(Arc::new(AtomicBool::new(false)));
        }
        let Some(cancel_token) = &self.scanner_cancel_token else {
            return;
        };

        let app_state_clone = Arc::clone(app_state);
        let spinner = spinner.clone();
        let progress_bar = self.progress_bar.clone();
        let progress_label = self.progress_label.clone();
        let cancel_button = self.cancel_button.clone();
        let add_button = self.add_button.clone();
        let message_label = self.message_label.clone();
        let description_label = self.description_label.clone();
        let is_scanning = Arc::clone(&self.is_scanning);
        let cancel_token_clone = Arc::clone(cancel_token);

        MainContext::default().spawn_local(async move {
            let rx = app_state_clone.subscribe();
            loop {
                select! {
                    result = rx.recv() => {
                        if let Ok(event) = result {
                            match event.as_ref() {
                                LibraryScanningChanged { is_scanning: true } => {
                                    add_button.set_visible(false);
                                    message_label.set_visible(false);
                                    description_label.set_visible(false);

                                    spinner.set_visible(true);
                                    spinner.set_size_request(48, 48);

                                    if let Some(bar) = &progress_bar {
                                        bar.set_visible(true);
                                        bar.set_fraction(0.0);
                                    }

                                    if let Some(label) = &progress_label {
                                        label.set_label("Scanning library...");
                                        label.set_visible(true);
                                    }

                                    if let Some(btn) = &cancel_button {
                                        btn.set_visible(true);
                                    }

                                    is_scanning.store(true, Relaxed);
                                }
                                LibraryScanningChanged { is_scanning: false } | LibraryDataChanged { .. } => {
                                    spinner.set_visible(false);

                                    if let Some(bar) = &progress_bar {
                                        bar.set_visible(false);
                                    }

                                    if let Some(label) = &progress_label {
                                        label.set_visible(false);
                                    }

                                    if let Some(btn) = &cancel_button {
                                        btn.set_visible(false);
                                    }

                                    is_scanning.store(false, Relaxed);
                                }
                                LibraryScanProgress { current, total } => {
                                    if let Some(bar) = &progress_bar {
                                        let fraction = if *total > 0 {
                                            current.to_f64().unwrap_or(0.0) / total.to_f64().unwrap_or(1.0)
                                        } else {
                                            0.0
                                        };
                                        bar.set_fraction(fraction);
                                    }

                                    if let Some(label) = &progress_label {
                                        label.set_label(&format!("Processing: {current}/{total} albums"));
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            debug!("Scanner subscription closed, stopping listener");
                            break;
                        }
                    }
                }
                if cancel_token_clone.load(Relaxed) {
                    debug!("Scanner event listener cancelled, resetting token and continuing");
                    cancel_token_clone.store(false, Relaxed);
                }
            }
        });
    }

    /// Prepares scanning resources including database connection, settings arc, and DR parser.
    ///
    /// # Arguments
    ///
    /// * `settings_arc` - Reference to the thread-safe settings Arc
    ///
    /// # Returns
    ///
    /// A tuple containing the database arc and optional DR parser
    async fn prepare_scan_resources(
        settings_arc: &Arc<RwLock<UserSettings>>,
    ) -> Option<(Arc<LibraryDatabase>, (), Option<Arc<DrParser>>)> {
        match LibraryDatabase::new().await {
            Ok(library_db) => {
                let library_db_arc = Arc::new(library_db);

                // Initialize DR parser if enabled
                let dr_parser = if settings_arc.read().show_dr_values {
                    match DrParser::new(Arc::clone(&library_db_arc)) {
                        Ok(parser) => Some(Arc::new(parser)),
                        Err(e) => {
                            error!(error = %e, "Failed to initialize DR parser");
                            None
                        }
                    }
                } else {
                    None
                };

                Some((library_db_arc, (), dr_parser))
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
    async fn start_scanner_event_listener(
        scanner_arc: &Arc<TokioRwLock<LibraryScanner>>,
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        cancel_token: &Arc<AtomicBool>,
    ) {
        // Start the event listener loop for the new scanner
        // This mirrors the logic in OxhidifiApplication::run
        let rx = scanner_arc.read().await.subscribe();
        let app_state_refresh = Arc::clone(app_state);
        let db_refresh = Arc::clone(library_db);
        let cancel_token_clone = Arc::clone(cancel_token);

        MainContext::default().spawn_local(async move {
            while !cancel_token_clone.load(Relaxed) {
                match rx.recv().await {
                    Ok(LibraryChanged) => {
                        debug!(
                            "LibraryChanged event received (dynamic scanner), refreshing app state"
                        );

                        // Refresh albums and artists in parallel
                        let (albums, artists) = match try_join!(
                            async {
                                db_refresh.get_albums(None).await.map_err(|e| {
                                    error!(error = %e, "Failed to refresh albums");
                                    e
                                })
                            },
                            async {
                                db_refresh.get_artists(None).await.map_err(|e| {
                                    error!(error = %e, "Failed to refresh artists");
                                    e
                                })
                            }
                        ) {
                            Ok((albums, artists)) => (albums, artists),
                            Err(_) => (Vec::new(), Vec::new()),
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
    async fn get_or_create_scanner(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_arc: &Arc<RwLock<UserSettings>>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Option<Arc<TokioRwLock<LibraryScanner>>> {
        let existing_scanner = app_state.library_scanner.read().clone();

        if let Some(scanner) = existing_scanner {
            return Some(scanner);
        }

        // Create new scanner
        match LibraryScanner::new(library_db, settings_arc, None) {
            Ok(scanner) => {
                let scanner_arc = Arc::new(TokioRwLock::new(scanner));

                // IMPORTANT: Store the scanner in AppState to prevent it from being dropped
                *app_state.library_scanner.write() = Some(Arc::clone(&scanner_arc));

                // Start the event listener loop for the new scanner
                // This mirrors the logic in OxhidifiApplication::run
                Self::start_scanner_event_listener(
                    &scanner_arc,
                    app_state,
                    library_db,
                    cancel_token,
                )
                .await;

                Some(scanner_arc)
            }
            Err(e) => {
                error!(error = %e, "Failed to create library scanner");
                None
            }
        }
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
    /// * `cancelled` - Cancellation token for stopping the scan
    /// * `app_state` - Application state for broadcasting progress updates
    async fn execute_background_scan(
        scanner: Arc<TokioRwLock<LibraryScanner>>,
        db: Arc<LibraryDatabase>,
        settings: Arc<RwLock<UserSettings>>,
        dr_parser: Option<Arc<DrParser>>,
        new_directory: String,
        cancelled: Arc<AtomicBool>,
        app_state: Option<Arc<AppState>>,
    ) -> bool {
        // Offload heavy scanning work to a background task
        // Clone for spawned task, keep original for error handling after
        let app_state_clone = app_state.as_ref().map(Arc::clone);
        let scan_handle = spawn(async move {
            // 1. Add directory to scanner (fast, takes write lock)
            {
                let mut scanner_write = scanner.write().await;
                if let Err(e) = scanner_write.add_library_directory(&new_directory) {
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
                let library_dirs = settings.read().library_directories.clone();
                let mut all_files = Vec::new();

                for dir in library_dirs {
                    let dir_path = Path::new(&dir);

                    // Paths are already canonicalized when added to settings, just validate they
                    // exist
                    if !dir_path.is_dir() {
                        warn!(path = %dir, "Library path is not a valid directory");
                        continue;
                    }

                    if let Ok(audio_files) =
                        LibraryScanner::collect_audio_files_from_directory(dir_path, &cancelled)
                    {
                        all_files.extend(audio_files);
                    }
                }
                all_files
            };

            // 3. Group files by album directory first (preserves album integrity)
            let mut files_by_album: HashMap<Arc<PathBuf>, Vec<Arc<PathBuf>>> = HashMap::new();
            for path in &all_audio_files {
                if let Some(parent) = path.parent() {
                    let parent_arc = Arc::new(parent.to_path_buf());
                    let path_arc = Arc::new(path.clone());
                    files_by_album.entry(parent_arc).or_default().push(path_arc);
                }
            }

            let total_albums = files_by_album.len();
            let progress_interval = SCAN_PROGRESS_INTERVAL;

            // 4. Process albums with progress updates
            for (processed_albums, (_album_dir, album_files)) in
                files_by_album.into_iter().enumerate()
            {
                let is_cancelled = cancelled.load(Relaxed)
                    || app_state_clone
                        .as_ref()
                        .is_some_and(|app| !app.is_scanning());

                if is_cancelled {
                    if let Some(app) = app_state_clone.as_ref() {
                        app.set_scanning(false);
                    }
                    return true;
                }

                let paths: Vec<PathBuf> =
                    album_files.into_iter().map(Arc::unwrap_or_clone).collect();

                if let Err(e) = handle_files_changed(paths, &db, dr_parser.as_ref()).await {
                    error!(error = %e, "Failed to process files");
                }

                let current = processed_albums + 1;

                if (current % progress_interval == 0 || current == total_albums)
                    && let Some(app) = app_state_clone.as_ref()
                {
                    app.broadcast_scan_progress(current, total_albums);
                }
            }

            if let Some(app) = app_state_clone.as_ref() {
                app.set_scanning(false);
            }

            false
        });

        match scan_handle.await {
            Ok(was_cancelled) => was_cancelled,
            Err(e) => {
                error!(error = %e, "Scan task panicked");
                if let Some(app) = app_state.as_ref() {
                    app.report_library_scan_failure(e.to_string());
                    app.set_scanning(false);
                }
                false
            }
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
        settings_manager: Arc<RwLock<SettingsManager>>,
        app_state: Option<Arc<AppState>>,
        cancel_token: Arc<AtomicBool>,
    ) {
        let settings_arc = settings_manager.read().get_settings_arc();

        if let Some(app_state) = app_state {
            app_state.set_scanning(true);

            let app_state_clone = Arc::clone(&app_state);
            let new_directory = new_directory.to_string();

            if let Some((library_db_arc, (), dr_parser)) =
                Self::prepare_scan_resources(&settings_arc).await
            {
                let scanner_arc = Self::get_or_create_scanner(
                    &app_state_clone,
                    &library_db_arc,
                    &settings_arc,
                    &cancel_token,
                )
                .await;

                if let Some(scanner) = scanner_arc {
                    let was_cancelled = Self::execute_background_scan(
                        scanner,
                        Arc::clone(&library_db_arc),
                        Arc::clone(&settings_arc),
                        dr_parser,
                        new_directory.clone(),
                        Arc::clone(&cancel_token),
                        Some(Arc::clone(&app_state)),
                    )
                    .await;

                    if was_cancelled {
                        if let Err(e) = settings_manager
                            .write()
                            .remove_library_directory(&new_directory)
                        {
                            error!(error = %e, "Failed to remove cancelled directory from settings");
                        }

                        if let Err(e) = library_db_arc
                            .remove_tracks_in_directory(&new_directory)
                            .await
                        {
                            error!(error = %e, "Failed to remove tracks from cancelled directory");
                        }
                    }

                    Self::refresh_library_ui_state(Arc::clone(&app_state_clone), library_db_arc)
                        .await;
                } else {
                    app_state_clone
                        .report_library_scan_failure("Failed to create scanner".to_string());
                    app_state_clone.set_scanning(false);
                }
            } else {
                app_state_clone
                    .report_library_scan_failure("Failed to prepare scan resources".to_string());
                app_state_clone.set_scanning(false);
            }

            app_state.set_scanning(false);
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

    use {
        anyhow::{Result, anyhow, bail},
        parking_lot::RwLock,
        tempfile::TempDir,
        tokio::sync::RwLock as TokioRwLock,
    };

    use crate::{
        audio::engine::AudioEngine,
        config::settings::{SettingsManager, UserSettings},
        library::{database::LibraryDatabase, dr_parser::DrParser, scanner::LibraryScanner},
        state::app_state::AppState,
        ui::components::empty_state::{EmptyState, EmptyStateConfig},
    };

    #[test]
    fn empty_state_config() {
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
    async fn prepare_scan_resources_with_dr_parser() -> Result<()> {
        let settings = UserSettings {
            show_dr_values: true,
            ..Default::default()
        };

        let settings_arc = Arc::new(RwLock::new(settings));
        let result = EmptyState::prepare_scan_resources(&settings_arc).await;

        let (_, (), dr_parser) = result.ok_or_else(|| anyhow!("result should be Some"))?;
        if dr_parser.is_none() {
            bail!("DR parser should be Some");
        }
        Ok(())
    }

    #[tokio::test]
    async fn prepare_scan_resources_without_dr_parser() -> Result<()> {
        let settings = UserSettings {
            show_dr_values: false,
            ..Default::default()
        };

        let settings_arc = Arc::new(RwLock::new(settings));
        let result = EmptyState::prepare_scan_resources(&settings_arc).await;

        let (_library_db, (), dr_parser) =
            result.ok_or_else(|| anyhow!("result should be Some"))?;

        if dr_parser.is_some() {
            bail!("DR parser should be None");
        }

        let settings_read = settings_arc.read();
        if settings_read.show_dr_values {
            bail!("Show DR values should be false");
        }
        drop(settings_read);
        Ok(())
    }

    #[tokio::test]
    async fn prepare_scan_resources_settings_arc_content() -> Result<()> {
        let test_dirs = vec!["/music/test".to_string(), "/another/path".to_string()];
        let settings = UserSettings {
            library_directories: test_dirs.clone(),
            show_dr_values: true,
            ..Default::default()
        };

        let settings_arc = Arc::new(RwLock::new(settings));
        let result = EmptyState::prepare_scan_resources(&settings_arc).await;

        let (_library_db, (), dr_parser) =
            result.ok_or_else(|| anyhow!("result should be Some"))?;

        let settings_read = settings_arc.read();
        if settings_read.library_directories != test_dirs {
            bail!("Library directories mismatch");
        }
        if !settings_read.show_dr_values {
            bail!("Show DR values should be true");
        }
        drop(settings_read);
        if dr_parser.is_none() {
            bail!("DR parser should be Some");
        }
        Ok(())
    }

    #[tokio::test]
    async fn get_or_create_scanner_returns_existing() -> Result<()> {
        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings::default();
        let settings_arc = Arc::new(RwLock::new(settings));

        let existing_scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let existing_scanner_arc = Arc::new(TokioRwLock::new(existing_scanner));

        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let app_state = Arc::new(AppState::new(
            engine_weak,
            Some(Arc::clone(&existing_scanner_arc)),
            Arc::new(RwLock::new(settings_manager)),
        ));

        let cancel_token = Arc::new(AtomicBool::new(false));

        let result = EmptyState::get_or_create_scanner(
            &app_state,
            &library_db_arc,
            &settings_arc,
            &cancel_token,
        )
        .await;

        let scanner_arc = result.ok_or_else(|| anyhow!("result should be Some"))?;
        if !Arc::ptr_eq(&scanner_arc, &existing_scanner_arc) {
            bail!("Should return existing scanner");
        }
        Ok(())
    }

    #[tokio::test]
    async fn get_or_create_scanner_creates_new() -> Result<()> {
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
    async fn get_or_create_scanner_stores_in_app_state() -> Result<()> {
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
        )
        .await;
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
    async fn execute_background_scan_adds_directory() -> Result<()> {
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
        let scanner_arc = Arc::new(TokioRwLock::new(scanner));

        let test_path = music_dir.to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            Arc::clone(&scanner_arc),
            library_db_arc,
            Arc::clone(&settings_arc),
            None,
            test_path,
            Arc::new(AtomicBool::new(false)),
            None,
        )
        .await;
        Ok(())
    }

    #[tokio::test]
    async fn execute_background_scan_with_files() -> Result<()> {
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
        let scanner_arc = Arc::new(TokioRwLock::new(scanner));

        let test_path = music_dir.to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            Arc::clone(&scanner_arc),
            Arc::clone(&library_db_arc),
            Arc::clone(&settings_arc),
            None,
            test_path,
            Arc::new(AtomicBool::new(false)),
            None,
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
    async fn execute_background_scan_with_dr_parser() -> Result<()> {
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
        let scanner_arc = Arc::new(TokioRwLock::new(scanner));

        let dr_parser = {
            let parser = DrParser::new(Arc::clone(&library_db_arc))?;
            Some(Arc::new(parser))
        };

        let test_path = music_dir.to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            Arc::clone(&scanner_arc),
            Arc::clone(&library_db_arc),
            Arc::clone(&settings_arc),
            dr_parser,
            test_path,
            Arc::new(AtomicBool::new(false)),
            None,
        )
        .await;
        Ok(())
    }

    #[tokio::test]
    async fn execute_background_scan_empty_library_directories() -> Result<()> {
        let library_db = LibraryDatabase::new().await?;
        let library_db_arc = Arc::new(library_db);

        let settings = UserSettings {
            library_directories: vec![],
            ..Default::default()
        };
        let settings_arc = Arc::new(RwLock::new(settings));

        let scanner = LibraryScanner::new(&library_db_arc, &settings_arc, None)?;
        let scanner_arc = Arc::new(TokioRwLock::new(scanner));

        let temp_dir = TempDir::new()?;
        let test_path = temp_dir.path().to_string_lossy().to_string();

        EmptyState::execute_background_scan(
            Arc::clone(&scanner_arc),
            Arc::clone(&library_db_arc),
            Arc::clone(&settings_arc),
            None,
            test_path,
            Arc::new(AtomicBool::new(false)),
            None,
        )
        .await;
        Ok(())
    }
}

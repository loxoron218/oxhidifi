//! Empty state UI component for when library grids or lists contain no content.
//!
//! This module implements the `EmptyState` component that displays a user-friendly
//! message with a button to add library directories when no albums or artists are available.

use std::{path::Path, sync::Arc};

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
    tracing::debug,
};

use crate::{
    config::SettingsManager,
    library::{
        database::LibraryDatabase,
        dr_parser::DrParser,
        scanner::{LibraryScanner, ScannerEvent::LibraryChanged, handlers::handle_files_changed},
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
        }
    }

    /// Connects event handlers to the add directory button.
    pub fn connect_button_handlers(&mut self) {
        let settings_manager_clone = self.settings_manager.clone();
        let window_clone = self.window.clone();
        let app_state_clone = self.app_state.clone();

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
                let dialog_clone = dialog.clone();
                let window_clone2 = window.clone();
                let settings_manager_clone2 = settings_manager_clone.clone();
                let app_state_clone2 = app_state_clone.clone();

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
                                            eprintln!("Failed to update settings: {e}");
                                            return;
                                        }

                                        // Log successful addition
                                        println!("Library directory added: {path_str}");

                                        // Trigger library rescan
                                        EmptyState::trigger_library_rescan(
                                            path_str,
                                            &settings_manager_clone,
                                            app_state_clone2.as_ref(),
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Folder selection cancelled or failed: {e}");
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

    /// Triggers a library rescan after adding a new directory.
    ///
    /// # Arguments
    ///
    /// * `new_directory` - The newly added directory path
    /// * `settings_manager` - Settings manager to get current settings
    /// * `app_state` - Application state to update UI
    async fn trigger_library_rescan(
        new_directory: &str,
        settings_manager: &SettingsManager,
        app_state: Option<&Arc<AppState>>,
    ) {
        if let Some(app_state) = app_state {
            let app_state_clone = app_state.clone();
            let new_directory = new_directory.to_string();
            let settings_snapshot = settings_manager.get_settings().clone();

            // Create a new database connection for scanning
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
                        Some(Arc::new(DrParser::new(library_db_arc.clone())))
                    } else {
                        None
                    };

                    // Check if we have an existing scanner or need to create one
                    let scanner_arc = {
                        let existing_scanner = app_state_clone.library_scanner.read().clone();
                        if let Some(scanner) = existing_scanner {
                            scanner
                        } else {
                            // Create new scanner
                            match LibraryScanner::new(
                                &library_db_arc.clone(),
                                &settings_arc.clone(),
                                None,
                            ) {
                                Ok(scanner) => {
                                    let scanner_arc = Arc::new(RwLock::new(scanner));

                                    // IMPORTANT: Store the scanner in AppState to prevent it from being dropped
                                    *app_state_clone.library_scanner.write() =
                                        Some(scanner_arc.clone());

                                    // Start the event listener loop for the new scanner
                                    // This mirrors the logic in OxhidifiApplication::run
                                    {
                                        let scanner_read = scanner_arc.read();
                                        let rx = scanner_read.subscribe();
                                        let app_state_refresh = app_state_clone.clone();
                                        let db_refresh = library_db_arc.clone();

                                        MainContext::default().spawn_local(async move {
                                            loop {
                                                match rx.recv().await {
                                                    Ok(LibraryChanged) => {
                                                        debug!("LibraryChanged event received (dynamic scanner), refreshing app state");

                                                        // Refresh albums
                                                        let albums = match db_refresh.get_albums(None).await {
                                                            Ok(albums) => albums,
                                                            Err(e) => {
                                                                eprintln!("Failed to refresh albums: {e}");
                                                                Vec::new()
                                                            }
                                                        };

                                                        // Refresh artists
                                                        let artists = match db_refresh.get_artists(None).await {
                                                            Ok(artists) => artists,
                                                            Err(e) => {
                                                                eprintln!("Failed to refresh artists: {e}");
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

                                    scanner_arc
                                }
                                Err(e) => {
                                    eprintln!("Failed to create library scanner: {e}");
                                    return;
                                }
                            }
                        }
                    };

                    // Offload heavy scanning work to a background task
                    let scanner_for_task = scanner_arc.clone();
                    let db_for_task = library_db_arc.clone();
                    let settings_for_task = settings_arc.clone();
                    let dr_parser_for_task = dr_parser.clone();
                    let dir_for_task = new_directory.clone();

                    let scan_handle = spawn(async move {
                        // 1. Add directory to scanner (fast, takes write lock)
                        {
                            let mut scanner_write = scanner_for_task.write();
                            if let Err(e) = scanner_write.add_library_directory(&dir_for_task) {
                                eprintln!("Failed to add directory to scanner: {e}");
                            }
                        }

                        // 2. Collect files (BLOCKING IO)
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
                            eprintln!("Failed to process files: {e}");
                        }
                    });

                    // Await the background task (yields to main loop so UI stays responsive)
                    if let Err(e) = scan_handle.await {
                        eprintln!("Scan task panicked: {e}");
                    }

                    // Update UI state with new library data
                    match library_db_arc.get_albums(None).await {
                        Ok(albums) => match library_db_arc.get_artists(None).await {
                            Ok(artists) => {
                                app_state_clone.update_library_data(albums, artists);
                            }
                            Err(e) => {
                                eprintln!("Failed to get artists from database: {e}");
                            }
                        },
                        Err(e) => {
                            eprintln!("Failed to get albums from database: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to create library database: {e}");
                }
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
    use crate::ui::components::empty_state::EmptyStateConfig;

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
}

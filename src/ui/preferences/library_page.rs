//! Library preferences page implementation.
//!
//! This module implements the Library preferences tab which handles
//! music library directory management and configuration.

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
        ApplicationWindow, PreferencesGroup, PreferencesPage,
        glib::MainContext,
        gtk::{
            AccessibleRole::Group, Align::Center, Box, Button, FileDialog, Label, ListBox,
            ListBoxRow, Orientation::Horizontal, ScrolledWindow,
            SelectionMode::None as SelectionNone, pango::EllipsizeMode::End,
        },
        prelude::{
            BoxExt, ButtonExt, Cast, FileExt, ListBoxRowExt, PreferencesGroupExt,
            PreferencesPageExt, WidgetExt,
        },
    },
    parking_lot::RwLock,
    tokio::{spawn, sync::RwLock as TokioRwLock, try_join},
    tracing::{debug, error, info, warn},
};

use crate::{
    config::settings::{SettingsManager, UserSettings},
    library::{
        database::LibraryDatabase,
        dr_parser::DrParser,
        scanner::{LibraryScanner, event_processing::handle_files_changed},
    },
    state::app_state::AppState,
    ui::components::empty_state::SCAN_PROGRESS_INTERVAL,
};

/// Library preferences page with directory management.
pub struct LibraryPreferencesPage {
    /// The underlying Libadwaita preferences page widget.
    pub widget: PreferencesPage,
    /// Application state reference for UI updates.
    app_state: Arc<AppState>,
    /// Library database reference for track management.
    library_db: Arc<LibraryDatabase>,
    /// Settings manager reference for persistence.
    settings_manager: Arc<RwLock<SettingsManager>>,
    /// List box for displaying library directories.
    directory_list_box: ListBox,
    /// Cancellation flag for library scans.
    is_scan_cancelled: Arc<AtomicBool>,
}

impl LibraryPreferencesPage {
    /// Creates a new library preferences page instance.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `library_db` - Library database reference for track management
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `LibraryPreferencesPage` instance.
    pub fn new(
        app_state: Arc<AppState>,
        library_db: Arc<LibraryDatabase>,
        settings_manager: Arc<RwLock<SettingsManager>>,
    ) -> Self {
        let widget = PreferencesPage::builder()
            .title("Library")
            .icon_name("folder-music-symbolic")
            .accessible_role(Group)
            .build();

        let page = Self {
            widget,
            app_state,
            library_db,
            settings_manager,
            directory_list_box: ListBox::new(),
            is_scan_cancelled: Arc::new(AtomicBool::new(false)),
        };

        page.setup_library_directories_group();
        page.refresh_directory_list();

        debug!("LibraryPreferencesPage: Created");

        page
    }

    /// Sets up the library directories management group.
    fn setup_library_directories_group(&self) {
        let group = PreferencesGroup::builder()
            .title("Music Library")
            .description("Manage directories containing your music collection")
            .build();

        // Add button to add new directory
        let add_button = Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add Directory")
            .has_frame(false)
            .build();

        let app_state_clone = Arc::clone(&self.app_state);
        let library_db_clone = Arc::clone(&self.library_db);
        let settings_manager_clone = Arc::clone(&self.settings_manager);
        let directory_list_box_clone = self.directory_list_box.clone();
        let is_scan_cancelled_clone = Arc::clone(&self.is_scan_cancelled);
        add_button.connect_clicked(move |button| {
            Self::show_add_directory_dialog(
                button,
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
                &is_scan_cancelled_clone,
            );
        });

        group.set_header_suffix(Some(&add_button));

        // Create scrolled window for directory list
        let scrolled_window = ScrolledWindow::builder()
            .vexpand(true)
            .min_content_height(200)
            .build();

        self.directory_list_box.set_selection_mode(SelectionNone);
        scrolled_window.set_child(Some(&self.directory_list_box));
        group.add(&scrolled_window);

        self.widget.add(&group);
    }

    /// Refreshes the directory list display from current settings.
    fn refresh_directory_list(&self) {
        // Clear existing rows
        let mut children = Vec::new();
        let mut child = self.directory_list_box.first_child();
        while let Some(c) = child {
            children.push(c.clone());
            child = c.next_sibling();
        }
        for child in children {
            self.directory_list_box.remove(&child);
        }

        // Add rows for each directory
        let directories = self
            .settings_manager
            .read()
            .get_settings()
            .library_directories
            .clone();
        for directory in &directories {
            let row = self.create_directory_row(directory);
            self.directory_list_box.append(&row);
        }

        if directories.is_empty() {
            let empty_label = Label::builder()
                .label("No library directories configured")
                .halign(Center)
                .valign(Center)
                .margin_top(24)
                .margin_bottom(24)
                .build();
            let empty_row = ListBoxRow::builder().selectable(false).build();
            empty_row.set_child(Some(&empty_label));
            self.directory_list_box.append(&empty_row);
        }
    }

    /// Creates a list box row for a specific directory.
    ///
    /// # Arguments
    ///
    /// * `directory` - Path string of the directory to display
    ///
    /// # Returns
    ///
    /// A `ListBoxRow` widget displaying the directory path with a remove button
    fn create_directory_row(&self, directory: &str) -> ListBoxRow {
        let row = ListBoxRow::builder().selectable(false).build();

        let main_box = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();

        let directory_label = Label::builder()
            .label(directory)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(End)
            .build();

        let remove_button = Button::builder()
            .icon_name("edit-delete-symbolic")
            .tooltip_text("Remove directory")
            .css_classes(["flat"])
            .use_underline(true)
            .build();

        let app_state_clone = Arc::clone(&self.app_state);
        let library_db_clone = Arc::clone(&self.library_db);
        let settings_manager_clone = Arc::clone(&self.settings_manager);
        let directory_list_box_clone = self.directory_list_box.clone();
        let is_scan_cancelled_clone = Arc::clone(&self.is_scan_cancelled);
        let directory_string = directory.to_string();
        remove_button.connect_clicked(move |_| {
            Self::remove_directory_from_settings(
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
                &is_scan_cancelled_clone,
                &directory_string,
            );
        });

        main_box.append(&directory_label);
        main_box.append(&remove_button);
        row.set_child(Some(&main_box));

        row
    }

    /// Shows a file chooser dialog to add a new directory.
    ///
    /// # Arguments
    ///
    /// * `button` - The button that triggered the dialog
    /// * `app_state` - Application state for scanner updates
    /// * `library_db` - Database for track management
    /// * `settings_manager` - Settings manager for persistence
    /// * `directory_list_box` - List box to refresh after adding
    /// * `is_scan_cancelled` - Cancellation flag for scan operations
    fn show_add_directory_dialog(
        button: &Button,
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
        is_scan_cancelled: &Arc<AtomicBool>,
    ) {
        let dialog = FileDialog::builder()
            .title("Select Music Folder")
            .accept_label("Add Folder")
            .modal(true)
            .build();

        let settings_manager_clone = Arc::clone(settings_manager);
        let directory_list_box_clone = directory_list_box.clone();
        let app_state_clone = Arc::clone(app_state);
        let library_db_clone = Arc::clone(library_db);
        let is_scan_cancelled_clone = Arc::clone(is_scan_cancelled);

        if let Some(root) = button.root()
            && let Some(window) = root.downcast_ref::<ApplicationWindow>()
        {
            let window = window.clone();
            MainContext::default().spawn_local(async move {
                let mut added_directory: Option<String> = None;

                match dialog.select_folder_future(Some(&window)).await {
                    Ok(folder) => {
                        if let Some(path) = folder.path()
                            && let Some(path_str) = path.to_str()
                        {
                            let path_string = path_str.to_string();
                            let path = Path::new(&path_string);

                            if !path.exists() {
                                warn!(path = %path_string, "Selected path does not exist");
                                return;
                            }

                            if !path.is_dir() {
                                warn!(path = %path_string, "Selected path is not a directory");
                                return;
                            }

                            let canonical_path = match path.canonicalize() {
                                Ok(p) => p,
                                Err(e) => {
                                    warn!(error = %e, path = %path_string, "Cannot access selected directory");
                                    return;
                                }
                            };

                            if !canonical_path.is_dir() {
                                warn!(path = %path_string, "Resolved path is not a valid directory");
                                return;
                            }

                            let canonical_path_string = canonical_path.to_string_lossy().to_string();

                            let settings_write = settings_manager_clone.write();
                            if let Err(e) = settings_write.add_library_directory(&canonical_path_string) {
                                error!(error = %e, "Failed to add library directory");
                                return;
                            }
                            drop(settings_write);

                            info!("Library directory added: {canonical_path_string}");
                            added_directory = Some(canonical_path_string);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Folder selection cancelled or failed");
                    }
                }

                Self::refresh_directory_list_from_settings(
                    &app_state_clone,
                    &library_db_clone,
                    &settings_manager_clone,
                    &directory_list_box_clone,
                    &is_scan_cancelled_clone,
                );

                if let Some(dir) = added_directory {
                    // Reset cancellation flag before triggering rescan
                    is_scan_cancelled_clone.store(false, Relaxed);

                    Self::trigger_library_rescan(
                        &app_state_clone,
                        &library_db_clone,
                        settings_manager_clone,
                        &is_scan_cancelled_clone,
                        Some(dir),
                    );
                }
            });
        } else {
            debug!("No parent window available for file dialog");
        }
    }

    /// Removes a directory from settings and refreshes the UI.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state for UI updates
    /// * `library_db` - Database for track removal
    /// * `settings_manager` - Settings manager for persistence
    /// * `directory_list_box` - List box to refresh after removal
    /// * `is_scan_cancelled` - Cancellation flag for scan operations
    /// * `directory_to_remove` - Path of the directory to remove
    fn remove_directory_from_settings(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
        is_scan_cancelled: &Arc<AtomicBool>,
        directory_to_remove: &str,
    ) {
        let directory_to_remove = directory_to_remove.to_string();
        let settings_manager_clone = Arc::clone(settings_manager);
        let directory_list_box_clone = directory_list_box.clone();
        let app_state_clone = Arc::clone(app_state);
        let library_db_clone = Arc::clone(library_db);
        let is_scan_cancelled_clone = Arc::clone(is_scan_cancelled);

        debug!("Removing directory: {}", directory_to_remove);

        // Cancel any ongoing scans before removing tracks
        is_scan_cancelled_clone.store(true, Relaxed);

        // Update settings synchronously
        {
            let settings_write = settings_manager_clone.write();
            if let Err(e) = settings_write.remove_library_directory(&directory_to_remove) {
                error!(error = %e, "Failed to remove directory from settings");
                return;
            }
        }

        MainContext::default().spawn_local(async move {
            // Remove tracks from database
            if let Err(e) = library_db_clone
                .remove_tracks_in_directory(&directory_to_remove)
                .await
            {
                error!(error = %e, "Failed to remove tracks from database");
            }

            // Refresh directory list UI
            Self::refresh_directory_list_from_settings(
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
                &is_scan_cancelled_clone,
            );

            // Refresh library UI (album/artist views) with incremental update
            match try_join!(
                library_db_clone.get_album_ids_by_directory(&directory_to_remove),
                library_db_clone.get_artist_ids_by_directory(&directory_to_remove)
            ) {
                Ok((album_ids, artist_ids)) => {
                    match try_join!(
                        library_db_clone.get_albums_by_ids(&album_ids),
                        library_db_clone.get_artists_by_ids(&artist_ids)
                    ) {
                        Ok((albums, artists)) => {
                            app_state_clone.update_library_data(albums, artists);
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to get albums or artists from database");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to get album or artist IDs from database");
                }
            }
        });
    }

    /// Refreshes the directory list from current settings.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `library_db` - Library database reference
    /// * `settings_manager` - Settings manager reference
    /// * `directory_list_box` - List box to refresh
    /// * `is_scan_cancelled` - Cancellation flag for scans
    fn refresh_directory_list_from_settings(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
        is_scan_cancelled: &Arc<AtomicBool>,
    ) {
        // Clear existing rows
        let mut children = Vec::new();
        let mut child = directory_list_box.first_child();
        while let Some(c) = child {
            children.push(c.clone());
            child = c.next_sibling();
        }
        for child in children {
            directory_list_box.remove(&child);
        }

        // Add rows for each directory
        let settings_read = settings_manager.read();
        let directories = settings_read.get_settings().library_directories.clone();
        drop(settings_read);

        for directory in &directories {
            let row = Self::create_standalone_directory_row(
                app_state,
                library_db,
                settings_manager,
                directory_list_box,
                is_scan_cancelled,
                directory,
            );
            directory_list_box.append(&row);
        }

        if directories.is_empty() {
            let empty_label = Label::builder()
                .label("No library directories configured")
                .halign(Center)
                .valign(Center)
                .margin_top(24)
                .margin_bottom(24)
                .build();
            directory_list_box.append(&empty_label);
        }
    }

    /// Creates a standalone directory row (without add button) for the library directories list.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `library_db` - Library database reference
    /// * `settings_manager` - Settings manager reference
    /// * `directory_list_box` - List box to add row to
    /// * `is_scan_cancelled` - Cancellation flag for scans
    /// * `directory` - Directory path to display
    fn create_standalone_directory_row(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
        is_scan_cancelled: &Arc<AtomicBool>,
        directory: &str,
    ) -> ListBoxRow {
        let row = ListBoxRow::builder().selectable(false).build();

        let main_box = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();

        let directory_label = Label::builder()
            .label(directory)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(End)
            .build();

        let remove_button = Button::builder()
            .icon_name("edit-delete-symbolic")
            .tooltip_text("Remove directory")
            .css_classes(["flat"])
            .use_underline(true)
            .build();

        let app_state_clone = Arc::clone(app_state);
        let library_db_clone = Arc::clone(library_db);
        let settings_manager_clone = Arc::clone(settings_manager);
        let directory_list_box_clone = directory_list_box.clone();
        let is_scan_cancelled_clone = Arc::clone(is_scan_cancelled);
        let directory_string = directory.to_string();
        remove_button.connect_clicked(move |_| {
            Self::remove_directory_from_settings(
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
                &is_scan_cancelled_clone,
                &directory_string,
            );
        });

        main_box.append(&directory_label);
        main_box.append(&remove_button);
        row.set_child(Some(&main_box));

        row
    }

    /// Triggers a library rescan by spawning a new scanner task.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state containing the library scanner reference
    /// * `library_db` - Database instance for accessing music library
    /// * `settings_manager` - Settings manager for scanner configuration
    /// * `cancel_token` - Cancellation token for stopping the scan
    /// * `new_directory` - Optional directory path to clean up if scan is cancelled
    fn trigger_library_rescan(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: Arc<RwLock<SettingsManager>>,
        cancel_token: &Arc<AtomicBool>,
        new_directory: Option<String>,
    ) {
        let app_state_clone = Arc::clone(app_state);
        let library_db_clone = Arc::clone(library_db);
        let cancel_token = Arc::clone(cancel_token);

        app_state_clone.set_scanning(true);

        spawn(async move {
            let settings_arc = Self::create_scanner_settings(&settings_manager);

            if let Err(e) =
                Self::ensure_scanner_initialized(&app_state_clone, &library_db_clone, &settings_arc)
            {
                error!(error = %e, "Failed to create library scanner");
                app_state_clone.report_library_scan_failure(e);
                app_state_clone.set_scanning(false);
                cancel_token.store(false, Relaxed);
                return;
            }

            let dr_parser = Self::create_dr_parser(&settings_arc, &library_db_clone);
            let all_audio_files = Self::collect_library_files(&settings_manager, &cancel_token);

            if all_audio_files.is_empty() {
                Self::finalize_scan(&app_state_clone, &library_db_clone, &cancel_token).await;
                return;
            }

            let (total_albums, processed, was_cancelled) = Self::process_albums(
                all_audio_files,
                &library_db_clone,
                &settings_arc,
                dr_parser.as_ref(),
                &cancel_token,
                &app_state_clone,
            )
            .await;

            if was_cancelled && let Some(dir) = &new_directory {
                if let Err(e) = settings_manager.write().remove_library_directory(dir) {
                    error!(error = %e, "Failed to remove cancelled directory from settings");
                }

                if let Err(e) = library_db_clone.remove_tracks_in_directory(dir).await {
                    error!(error = %e, "Failed to remove tracks from cancelled directory");
                }
            }

            debug!(
                albums_processed = processed,
                total_albums = total_albums,
                was_cancelled = was_cancelled,
                "Album processing completed"
            );
            Self::finalize_scan(&app_state_clone, &library_db_clone, &cancel_token).await;
        });
    }

    /// Creates scanner settings from the settings manager.
    ///
    /// # Arguments
    ///
    /// * `settings_manager` - Settings manager for accessing scanner configuration
    ///
    /// # Returns
    ///
    /// A new `Arc<RwLock<UserSettings>>` containing library directories and DR preferences
    fn create_scanner_settings(
        settings_manager: &Arc<RwLock<SettingsManager>>,
    ) -> Arc<RwLock<UserSettings>> {
        let (library_dirs, show_dr) = {
            let settings_read = settings_manager.read();
            settings_read.get_scanner_settings()
        };

        Arc::new(RwLock::new(UserSettings {
            library_directories: library_dirs,
            show_dr_values: show_dr,
            ..Default::default()
        }))
    }

    /// Ensures the library scanner is initialized.
    ///
    /// # Arguments
    ///
    /// * `app_state_clone` - Application state for storing the scanner
    /// * `library_db_clone` - Database for scanner operations
    /// * `settings_arc` - User settings for scanner configuration
    ///
    /// # Returns
    ///
    /// A `Result` with an empty unit on success or a `String` error message on failure
    fn ensure_scanner_initialized(
        app_state_clone: &Arc<AppState>,
        library_db_clone: &Arc<LibraryDatabase>,
        settings_arc: &Arc<RwLock<UserSettings>>,
    ) -> Result<(), String> {
        {
            let mut scanner_guard = app_state_clone.library_scanner.write();
            if scanner_guard.is_some() {
                return Ok(());
            }

            let scanner = LibraryScanner::new(library_db_clone, settings_arc, None)
                .map_err(|e| e.to_string())?;
            *scanner_guard = Some(Arc::new(TokioRwLock::new(scanner)));
        }

        Ok(())
    }

    /// Creates a DR parser if enabled in settings.
    ///
    /// # Arguments
    ///
    /// * `settings_arc` - User settings for checking DR preference
    /// * `library_db_clone` - Database for the DR parser
    ///
    /// # Returns
    ///
    /// An `Option` containing the DR parser if enabled, or `None` otherwise
    fn create_dr_parser(
        settings_arc: &Arc<RwLock<UserSettings>>,
        library_db_clone: &Arc<LibraryDatabase>,
    ) -> Option<Arc<DrParser>> {
        if !settings_arc.read().show_dr_values {
            return None;
        }

        match DrParser::new(Arc::clone(library_db_clone)) {
            Ok(parser) => Some(Arc::new(parser)),
            Err(e) => {
                error!(error = %e, "Failed to create DR parser");
                None
            }
        }
    }

    /// Collects all audio files from library directories.
    ///
    /// # Arguments
    ///
    /// * `settings_manager` - Settings manager for accessing library directories
    /// * `cancel_token` - Cancellation token for stopping the scan
    ///
    /// # Returns
    ///
    /// A `Vec<PathBuf>` containing all audio file paths found in the library
    fn collect_library_files(
        settings_manager: &Arc<RwLock<SettingsManager>>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Vec<PathBuf> {
        let library_dirs = settings_manager.read().get_library_directories();
        let mut all_files = Vec::with_capacity(library_dirs.len() * 500);

        for dir in &library_dirs {
            let dir_path = Path::new(dir);

            if !dir_path.is_dir() {
                warn!(path = %dir, "Library path is not a valid directory");
                continue;
            }

            if let Ok(audio_files) =
                LibraryScanner::collect_audio_files_from_directory(dir_path, cancel_token)
            {
                all_files.extend(audio_files);
            }
        }

        all_files
    }

    /// Processes all albums and updates the database.
    ///
    /// # Arguments
    ///
    /// * `all_audio_files` - List of all audio file paths to process
    /// * `library_db_clone` - Database for metadata storage
    /// * `settings_arc` - User settings for processing configuration
    /// * `dr_parser` - Optional DR parser for dynamic range values
    /// * `cancel_token` - Cancellation token for stopping the scan
    /// * `app_state_clone` - Application state for progress updates
    ///
    /// # Returns
    ///
    /// A tuple of `(total_albums, processed_count)`
    async fn process_albums(
        all_audio_files: Vec<PathBuf>,
        library_db_clone: &Arc<LibraryDatabase>,
        settings_arc: &Arc<RwLock<UserSettings>>,
        dr_parser: Option<&Arc<DrParser>>,
        cancel_token: &Arc<AtomicBool>,
        app_state_clone: &Arc<AppState>,
    ) -> (usize, usize, bool) {
        let mut files_by_album: HashMap<Arc<PathBuf>, Vec<Arc<PathBuf>>> = HashMap::new();
        for path in all_audio_files {
            if let Some(parent) = path.parent() {
                let parent_arc = Arc::new(parent.to_path_buf());
                let path_arc = Arc::new(path);
                files_by_album.entry(parent_arc).or_default().push(path_arc);
            }
        }

        let total_albums = files_by_album.len();
        let mut processed = 0;
        let progress_interval = SCAN_PROGRESS_INTERVAL;

        for album_files in files_by_album.into_values() {
            if cancel_token.load(Relaxed) || !app_state_clone.is_scanning() {
                return (total_albums, processed, true);
            }

            let paths: Vec<PathBuf> = album_files.into_iter().map(Arc::unwrap_or_clone).collect();
            if let Err(e) =
                handle_files_changed(paths, library_db_clone, settings_arc, dr_parser).await
            {
                error!(error = %e, "Failed to process files");
            }

            processed += 1;
            if processed % progress_interval == 0 || processed == total_albums {
                app_state_clone.broadcast_scan_progress(processed, total_albums);
            }
        }

        (total_albums, processed, false)
    }

    /// Finalizes the scan by fetching updated library data and updating app state.
    ///
    /// # Arguments
    ///
    /// * `app_state_clone` - Application state for updating library data
    /// * `library_db_clone` - Database for fetching updated albums and artists
    /// * `cancel_token` - Cancellation token to reset after scan completes
    async fn finalize_scan(
        app_state_clone: &Arc<AppState>,
        library_db_clone: &Arc<LibraryDatabase>,
        cancel_token: &Arc<AtomicBool>,
    ) {
        match try_join!(
            library_db_clone.get_all_album_ids(),
            library_db_clone.get_all_artist_ids()
        ) {
            Ok((album_ids, artist_ids)) => {
                match try_join!(
                    library_db_clone.get_albums_by_ids(&album_ids),
                    library_db_clone.get_artists_by_ids(&artist_ids)
                ) {
                    Ok((albums, artists)) => {
                        app_state_clone.update_library_data(albums, artists);
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to get albums or artists from database");
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to get album or artist IDs from database");
            }
        }

        app_state_clone.set_scanning(false);

        // Reset cancellation flag for future scans
        cancel_token.store(false, Relaxed);
    }
}

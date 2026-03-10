//! Library preferences page implementation.
//!
//! This module implements the Library preferences tab which handles
//! music library directory management and configuration.

use std::{path::Path, sync::Arc};

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
    tokio::{spawn, try_join},
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

        let app_state_clone = self.app_state.clone();
        let library_db_clone = self.library_db.clone();
        let settings_manager_clone = self.settings_manager.clone();
        let directory_list_box_clone = self.directory_list_box.clone();
        add_button.connect_clicked(move |button| {
            Self::show_add_directory_dialog(
                button,
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
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

        let app_state_clone = self.app_state.clone();
        let library_db_clone = self.library_db.clone();
        let settings_manager_clone = self.settings_manager.clone();
        let directory_list_box_clone = self.directory_list_box.clone();
        let directory_string = directory.to_string();
        remove_button.connect_clicked(move |_| {
            Self::remove_directory_from_settings(
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
                &directory_string,
            );
        });

        main_box.append(&directory_label);
        main_box.append(&remove_button);
        row.set_child(Some(&main_box));

        row
    }

    /// Shows a file chooser dialog to add a new directory.
    fn show_add_directory_dialog(
        button: &Button,
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
    ) {
        let dialog = FileDialog::builder()
            .title("Select Music Folder")
            .accept_label("Add Folder")
            .modal(true)
            .build();

        let settings_manager_clone = settings_manager.clone();
        let directory_list_box_clone = directory_list_box.clone();
        let app_state_clone = app_state.clone();
        let library_db_clone = library_db.clone();

        if let Some(root) = button.root()
            && let Some(window) = root.downcast_ref::<ApplicationWindow>()
        {
            let window = window.clone();
            MainContext::default().spawn_local(async move {
                let mut should_rescan = false;

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
                                error!("Failed to add library directory: {e}");
                                return;
                            }
                            drop(settings_write);

                            should_rescan = true;
                            info!("Library directory added: {canonical_path_string}");
                        }
                    }
                    Err(e) => {
                        warn!("Folder selection cancelled or failed: {e}");
                    }
                }

                Self::refresh_directory_list_from_settings(
                    &app_state_clone,
                    &library_db_clone,
                    &settings_manager_clone,
                    &directory_list_box_clone,
                );

                if should_rescan {
                    Self::trigger_library_rescan(
                        &app_state_clone,
                        &library_db_clone,
                        settings_manager_clone,
                    );
                }
            });
        } else {
            debug!("No parent window available for file dialog");
        }
    }

    /// Removes a directory from settings and refreshes the UI.
    fn remove_directory_from_settings(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
        directory_to_remove: &str,
    ) {
        let directory_to_remove = directory_to_remove.to_string();
        let settings_manager_clone = settings_manager.clone();
        let directory_list_box_clone = directory_list_box.clone();
        let app_state_clone = app_state.clone();
        let library_db_clone = library_db.clone();

        debug!("Removing directory: {}", directory_to_remove);

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
    fn refresh_directory_list_from_settings(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
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
            let empty_row = ListBoxRow::builder().selectable(false).build();
            empty_row.set_child(Some(&empty_label));
            directory_list_box.append(&empty_row);
        }
    }

    /// Creates a standalone directory row (for static method usage).
    fn create_standalone_directory_row(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
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

        let app_state_clone = app_state.clone();
        let library_db_clone = library_db.clone();
        let settings_manager_clone = settings_manager.clone();
        let directory_list_box_clone = directory_list_box.clone();
        let directory_string = directory.to_string();
        remove_button.connect_clicked(move |_| {
            Self::remove_directory_from_settings(
                &app_state_clone,
                &library_db_clone,
                &settings_manager_clone,
                &directory_list_box_clone,
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
    fn trigger_library_rescan(
        app_state: &Arc<AppState>,
        library_db: &Arc<LibraryDatabase>,
        settings_manager: Arc<RwLock<SettingsManager>>,
    ) {
        let app_state_clone = app_state.clone();
        let library_db_clone = library_db.clone();

        spawn(async move {
            let (library_dirs, show_dr) = {
                let settings_read = settings_manager.read();
                settings_read.get_scanner_settings()
            };

            let settings_arc = Arc::new(RwLock::new(UserSettings {
                library_directories: library_dirs.clone(),
                show_dr_values: show_dr,
                ..Default::default()
            }));

            // Check if scanner already exists, use write lock to prevent TOCTOU race
            // Scope the guard so it's dropped before async operations
            {
                let mut scanner_guard = app_state_clone.library_scanner.write();
                if scanner_guard.is_none() {
                    // Create new scanner if none exists
                    match LibraryScanner::new(&library_db_clone, &settings_arc, None) {
                        Ok(scanner) => {
                            let new_scanner = Arc::new(RwLock::new(scanner));
                            *scanner_guard = Some(new_scanner);
                            drop(scanner_guard);
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to create library scanner");
                            app_state_clone.report_library_scan_failure(e.to_string());
                            return;
                        }
                    }
                }
            }

            // Create DR parser if enabled in settings
            let dr_parser = if settings_arc.read().show_dr_values {
                match DrParser::new(library_db_clone.clone()) {
                    Ok(parser) => Some(Arc::new(parser)),
                    Err(e) => {
                        error!(error = %e, "Failed to create DR parser");
                        None
                    }
                }
            } else {
                None
            };

            // Collect files in a separate scope to drop the scanner read guard
            let all_audio_files = {
                let library_dirs = settings_arc.read().library_directories.clone();
                let mut all_files = Vec::with_capacity(library_dirs.len() * 500);

                for dir in library_dirs {
                    let dir_path = Path::new(&dir);

                    // Paths are already canonicalized when added to settings, just validate they exist
                    if !dir_path.is_dir() {
                        warn!(path = %dir, "Library path is not a valid directory");
                        continue;
                    }

                    if let Ok(audio_files) =
                        LibraryScanner::collect_audio_files_from_directory(dir_path)
                    {
                        all_files.extend(audio_files);
                    }
                }
                all_files
            };

            // Process files with the scanner (drop scanner guard first)
            if !all_audio_files.is_empty() {
                let db_ref: &LibraryDatabase = &library_db_clone;
                let process_result =
                    handle_files_changed(all_audio_files, db_ref, &settings_arc, &dr_parser).await;

                if let Err(e) = process_result {
                    error!(error = %e, "Failed to process files");
                }
            }

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
        });
    }
}

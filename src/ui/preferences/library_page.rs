//! Library preferences page implementation.
//!
//! This module implements the Library preferences tab which handles
//! music library directory management and configuration.

use std::sync::Arc;

use {
    libadwaita::{
        ApplicationWindow, PreferencesGroup, PreferencesPage,
        glib::MainContext,
        gtk::{
            AccessibleRole::Group, Align::Center, Box, Button, FileDialog, Label, ListBox,
            ListBoxRow, Orientation::Horizontal, ScrolledWindow, SelectionMode::None,
            pango::EllipsizeMode::End,
        },
        prelude::{
            BoxExt, ButtonExt, Cast, FileExt, ListBoxRowExt, PreferencesGroupExt,
            PreferencesPageExt, WidgetExt,
        },
    },
    parking_lot::RwLock,
    tracing::{debug, error, info, warn},
};

use crate::{config::settings::SettingsManager, state::app_state::AppState};

/// Library preferences page with directory management.
pub struct LibraryPreferencesPage {
    /// The underlying Libadwaita preferences page widget.
    pub widget: PreferencesPage,
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
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `LibraryPreferencesPage` instance.
    pub fn new(_app_state: Arc<AppState>, settings_manager: Arc<RwLock<SettingsManager>>) -> Self {
        let widget = PreferencesPage::builder()
            .title("Library")
            .icon_name("folder-music-symbolic")
            .accessible_role(Group)
            .build();

        let page = Self {
            widget,
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
            .label("Add Directory")
            .css_classes(["suggested-action"])
            .use_underline(true)
            .build();

        let settings_manager_clone = self.settings_manager.clone();
        let directory_list_box_clone = self.directory_list_box.clone();
        add_button.connect_clicked(move |button| {
            Self::show_add_directory_dialog(
                button,
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

        self.directory_list_box.set_selection_mode(None);
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

        let settings_manager_clone = self.settings_manager.clone();
        let directory_list_box_clone = self.directory_list_box.clone();
        let directory_string = directory.to_string();
        remove_button.connect_clicked(move |_| {
            Self::remove_directory_from_settings(
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

        if let Some(root) = button.root()
            && let Some(window) = root.downcast_ref::<ApplicationWindow>()
        {
            let window = window.clone();
            MainContext::default().spawn_local(async move {
                match dialog.select_folder_future(Some(&window)).await {
                    Ok(folder) => {
                        if let Some(path) = folder.path()
                            && let Some(path_str) = path.to_str()
                        {
                            let path_string = path_str.to_string();

                            let settings_read = settings_manager_clone.read();
                            let mut current_settings = settings_read.get_settings().clone();
                            drop(settings_read);

                            if !current_settings.library_directories.contains(&path_string) {
                                current_settings
                                    .library_directories
                                    .push(path_string.clone());

                                let settings_write = settings_manager_clone.write();
                                if let Err(e) = settings_write.update_settings(current_settings) {
                                    error!("Failed to update settings: {e}");
                                    return;
                                }
                                drop(settings_write);

                                info!("Library directory added: {path_string}");
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Folder selection cancelled or failed: {e}");
                    }
                }

                Self::refresh_directory_list_from_settings(
                    &settings_manager_clone,
                    &directory_list_box_clone,
                );
            });
        } else {
            debug!("No parent window available for file dialog");
        }
    }

    /// Removes a directory from settings and refreshes the UI.
    fn remove_directory_from_settings(
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
        directory_to_remove: &str,
    ) {
        debug!("Removing directory: {}", directory_to_remove);

        let settings_read = settings_manager.read();
        let mut current_settings = settings_read.get_settings().clone();
        drop(settings_read);

        current_settings
            .library_directories
            .retain(|dir| dir != directory_to_remove);

        let settings_write = settings_manager.write();
        if let Err(e) = settings_write.update_settings(current_settings) {
            error!(error = %e, "Failed to remove directory from settings");
            return;
        }
        drop(settings_write);

        Self::refresh_directory_list_from_settings(settings_manager, directory_list_box);
    }

    /// Refreshes the directory list from current settings.
    fn refresh_directory_list_from_settings(
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
                directory,
                settings_manager,
                directory_list_box,
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
        directory: &str,
        settings_manager: &Arc<RwLock<SettingsManager>>,
        directory_list_box: &ListBox,
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

        let settings_manager_clone = settings_manager.clone();
        let directory_list_box_clone = directory_list_box.clone();
        let directory_string = directory.to_string();
        remove_button.connect_clicked(move |_| {
            Self::remove_directory_from_settings(
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
}

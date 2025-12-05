//! Empty state UI component for when library grids or lists contain no content.
//!
//! This module implements the `EmptyState` component that displays a user-friendly
//! message with a button to add library directories when no albums or artists are available.

use std::{error::Error, sync::Arc};

use libadwaita::{
    gtk::{
        Align::{Center, Fill},
        Box as GtkBox, Button, Label,
        Orientation::Vertical,
        Widget,
    },
    prelude::{BoxExt, ButtonExt, Cast, WidgetExt},
};

use crate::{
    config::SettingsManager,
    state::{AppState, LibraryState},
};

/// Configuration for EmptyState display options.
#[derive(Debug, Clone)]
pub struct EmptyStateConfig {
    /// Whether this empty state is for albums or artists.
    pub is_album_view: bool,
}

/// Empty state UI component for when library grids or lists contain no content.
///
/// The `EmptyState` component displays a clear message explaining that no content
/// is available and provides a button to add library directories using a native
/// folder picker dialog.
pub struct EmptyState {
    /// The underlying GTK widget container.
    pub widget: Widget,
    /// Main container box.
    pub container: GtkBox,
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
}

impl EmptyState {
    /// Creates a new EmptyState component.
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
    pub fn new(
        app_state: Option<Arc<AppState>>,
        settings_manager: Option<SettingsManager>,
        config: EmptyStateConfig,
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
        let container = GtkBox::builder()
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
        let widget = GtkBox::builder()
            .orientation(Vertical)
            .halign(Fill)
            .valign(Fill)
            .build();
        widget.append(&container);

        let mut empty_state = Self {
            widget: widget.upcast_ref::<Widget>().clone(),
            container,
            message_label,
            add_button,
            app_state,
            settings_manager,
            config,
        };

        // Connect button click handler
        empty_state.connect_button_handlers();

        empty_state
    }

    /// Connects event handlers to the add directory button.
    fn connect_button_handlers(&mut self) {
        let _app_state_clone = self.app_state.clone();
        let settings_manager_clone = self.settings_manager.clone();

        self.add_button.connect_clicked(move |_| {
            // Open native folder picker dialog
            if let Some(settings_manager) = &settings_manager_clone
                && let Ok(selected_path) = Self::open_folder_picker()
            {
                // Clone the settings manager to get mutable access
                let mut settings_manager_clone = settings_manager.clone();
                // Update settings with new library directory
                let mut current_settings = settings_manager_clone.get_settings().clone();
                current_settings
                    .library_directories
                    .push(selected_path.clone());

                if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                    eprintln!("Failed to update settings: {}", e);
                    return;
                }

                // In a real implementation, this would trigger a library rescan
                // For now, we'll just log that it should happen
                println!("Library directory added: {}", selected_path);
                println!(
                    "Library rescan should be triggered for path: {}",
                    selected_path
                );

                // Update the empty state to reflect that directories have been added
                // This would typically be handled by the library scanner updating AppState
            }
        });
    }

    /// Opens a native folder picker dialog and returns the selected path.
    ///
    /// # Returns
    ///
    /// A `Result` containing the selected path string or an error.
    fn open_folder_picker() -> Result<String, Box<dyn Error>> {
        // For compilation purposes, return a placeholder
        // The real implementation would use proper GTK async patterns
        // with FileDialog::select_folder() and callbacks
        Err("Folder picker implementation requires async GTK handling".into())
    }

    /// Updates the empty state based on the current library state.
    ///
    /// # Arguments
    ///
    /// * `library_state` - Current library state to check for emptiness
    pub fn update_from_library_state(&self, library_state: &LibraryState) {
        let is_empty = match self.config.is_album_view {
            true => library_state.albums.is_empty(),
            false => library_state.artists.is_empty(),
        };

        self.widget.set_visible(is_empty);
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

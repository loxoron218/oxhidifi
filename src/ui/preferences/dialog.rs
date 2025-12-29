//! Main preferences dialog implementation.
//!
//! This module implements the `PreferencesDialog` which serves as the
//! main container for all preference settings organized into three tabs:
//! General, Library, and Audio.

use std::sync::Arc;

use {
    libadwaita::{
        ApplicationWindow, PreferencesDialog as LibadwaitaPreferencesDialog,
        gtk::Widget,
        prelude::{AdwDialogExt, PreferencesDialogExt},
    },
    tracing::debug,
};

use crate::{
    config::SettingsManager,
    state::AppState,
    ui::preferences::{AudioPreferencesPage, GeneralPreferencesPage, LibraryPreferencesPage},
};

/// Main preferences dialog with three categorized tabs.
///
/// The `PreferencesDialog` provides a GNOME HIG-compliant interface for
/// managing application settings across three categories: General, Library,
/// and Audio preferences.
pub struct PreferencesDialog {
    /// The underlying Libadwaita preferences dialog widget.
    pub widget: LibadwaitaPreferencesDialog,
}

impl PreferencesDialog {
    /// Creates a new preferences dialog instance.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for reactive updates
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `PreferencesDialog` instance.
    pub fn new(app_state: Arc<AppState>, settings_manager: Arc<SettingsManager>) -> Self {
        let widget = LibadwaitaPreferencesDialog::builder().build();

        // Set fixed dialog dimensions for consistent layout across all form factors
        widget.set_content_width(900);
        widget.set_content_height(700);

        // Create and add General preferences page
        let general_page = GeneralPreferencesPage::new(app_state.clone(), settings_manager.clone());
        widget.add(&general_page.widget);

        // Create and add Library preferences page
        let library_page = LibraryPreferencesPage::new(app_state.clone(), settings_manager.clone());
        widget.add(&library_page.widget);

        // Create and add Audio preferences page
        let audio_page = AudioPreferencesPage::new(app_state.clone(), settings_manager.clone());
        widget.add(&audio_page.widget);

        debug!("PreferencesDialog: Created with three tabs");

        Self { widget }
    }

    /// Shows the preferences dialog with a parent window.
    ///
    /// # Arguments
    ///
    /// * `parent` - Parent window widget for proper modal behavior
    pub fn show(&self, parent: &ApplicationWindow) {
        debug!("PreferencesDialog: Showing dialog with parent");
        self.widget.present(Some(parent));
    }

    /// Shows the preferences dialog without a parent window.
    pub fn show_without_parent(&self) {
        debug!("PreferencesDialog: Showing dialog without parent");
        self.widget.present(None::<&Widget>);
    }
}

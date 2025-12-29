//! General preferences page implementation.
//!
//! This module implements the General preferences tab which includes
//! theme preference, DR values display, and default view mode settings.

use std::sync::Arc;

use {
    libadwaita::{
        ComboRow, PreferencesGroup, PreferencesPage, SwitchRow,
        gtk::{AccessibleRole::Group, StringList},
        prelude::{ComboRowExt, PreferencesGroupExt, PreferencesPageExt},
    },
    tracing::debug,
};

use crate::{config::SettingsManager, state::AppState};

/// General preferences page with theme, DR values, and view mode settings.
pub struct GeneralPreferencesPage {
    /// The underlying Libadwaita preferences page widget.
    pub widget: PreferencesPage,
    /// Application state reference.
    app_state: Arc<AppState>,
    /// Settings manager reference for persistence.
    settings_manager: Arc<SettingsManager>,
}

impl GeneralPreferencesPage {
    /// Creates a new general preferences page instance.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `GeneralPreferencesPage` instance.
    pub fn new(app_state: Arc<AppState>, settings_manager: Arc<SettingsManager>) -> Self {
        let widget = PreferencesPage::builder()
            .title("General")
            .icon_name("preferences-system-symbolic")
            .accessible_role(Group)
            .build();

        let mut page = Self {
            widget,
            app_state,
            settings_manager,
        };

        page.setup_theme_preference();
        page.setup_dr_values_preference();

        debug!("GeneralPreferencesPage: Created");

        page
    }

    /// Sets up the theme preference combo row.
    fn setup_theme_preference(&mut self) {
        let group = PreferencesGroup::builder()
            .title("Appearance")
            .description("Customize the application's visual appearance")
            .build();

        // Create theme options
        let themes = vec!["System", "Light", "Dark"];
        let current_theme = self
            .settings_manager
            .get_settings()
            .theme_preference
            .clone();

        let combo_row = ComboRow::builder()
            .title("Theme")
            .subtitle("Choose light or dark theme, or follow system preference")
            .build();

        // Create string list for combo row
        let string_list = StringList::new(&themes);
        combo_row.set_model(Some(&string_list));

        // Set current selection
        let current_index = match current_theme.as_str() {
            "system" => 0,
            "light" => 1,
            "dark" => 2,
            _ => 0,
        };
        combo_row.set_selected(current_index as u32);

        // Connect change handler
        let settings_manager_clone = self.settings_manager.clone();
        combo_row.connect_selected_notify(move |row| {
            let selected_index = row.selected() as usize;
            let new_theme = match selected_index {
                0 => "system".to_string(),
                1 => "light".to_string(),
                2 => "dark".to_string(),
                _ => "system".to_string(),
            };

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.theme_preference = new_theme;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                debug!("Failed to update theme preference: {}", e);
            }
        });

        group.add(&combo_row);
        self.widget.add(&group);
    }

    /// Sets up the DR values display switch row.
    fn setup_dr_values_preference(&mut self) {
        let group = PreferencesGroup::builder()
            .title("Dynamic Range")
            .description("Display DR (Dynamic Range) values on album covers")
            .build();

        let current_show_dr = self.settings_manager.get_settings().show_dr_values;

        let switch_row = SwitchRow::builder()
            .title("Show DR Values")
            .subtitle("Display Dynamic Range quality indicators on album artwork")
            .active(current_show_dr)
            .build();

        // Connect change handler
        let settings_manager_clone = self.settings_manager.clone();
        let app_state_clone = self.app_state.clone();
        switch_row.connect_active_notify(move |row| {
            let new_value = row.is_active();

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.show_dr_values = new_value;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                debug!("Failed to update DR values preference: {}", e);
                return;
            }

            // Notify app state of the change using the proper settings update method
            app_state_clone.update_show_dr_values_setting(new_value);
        });

        group.add(&switch_row);
        self.widget.add(&group);
    }
}

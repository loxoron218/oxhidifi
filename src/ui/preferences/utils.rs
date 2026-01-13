//! Shared utility functions for preferences pages.
//!
//! This module provides utility functions that can be reused across
//! different preference page implementations.

use std::{string::String, sync::Arc};

use libadwaita::{ComboRow, gtk::StringList, prelude::ComboRowExt};

use crate::config::{SettingsManager, UserSettings};

/// Creates a combo row from settings with automatic persistence.
///
/// This function creates a `ComboRow` with the given title and options,
/// sets the current selection based on settings, and automatically
/// persists changes back to the settings manager.
///
/// # Arguments
///
/// * `title` - The title for the combo row
/// * `subtitle` - Optional subtitle for the combo row
/// * `options` - Vector of string options to display
/// * `current_value` - Current value from settings
/// * `getter` - Function to get the current value from settings
/// * `setter` - Function to set the new value in settings
/// * `settings_manager` - Settings manager reference for persistence
///
/// # Returns
///
/// A configured `ComboRow` ready to be added to a preferences group.
pub fn create_combo_row_from_settings<F, G>(
    title: &str,
    subtitle: Option<&str>,
    options: Vec<String>,
    _current_value: String,
    getter: F,
    setter: G,
    settings_manager: Arc<SettingsManager>,
) -> ComboRow
where
    F: Fn(&UserSettings) -> String + 'static,
    G: Fn(&mut UserSettings, String) + 'static,
{
    let combo_row = if let Some(sub) = subtitle {
        ComboRow::builder().title(title).subtitle(sub).build()
    } else {
        ComboRow::builder().title(title).build()
    };

    // Create string list for combo row
    let string_refs: Vec<&str> = options.iter().map(String::as_str).collect();
    let string_list = StringList::new(&string_refs);
    combo_row.set_model(Some(&string_list));

    // Find and set current selection
    if let Some(current_index) = options.iter().position(|opt| {
        let current_from_settings = getter(&settings_manager.get_settings());
        opt == &current_from_settings
    }) {
        combo_row.set_selected(current_index as u32);
    }

    // Connect change handler
    combo_row.connect_selected_notify(move |row| {
        let selected_index = row.selected() as usize;
        if selected_index < options.len() {
            let new_value = options[selected_index].clone();

            // Update settings
            let mut current_settings = settings_manager.get_settings().clone();
            setter(&mut current_settings, new_value);

            let _ = settings_manager.update_settings(current_settings);
        }
    });

    combo_row
}

#[cfg(test)]
mod tests {
    use std::{fs::remove_file, path::PathBuf, sync::Arc};

    use libadwaita::prelude::{ActionRowExt, PreferencesRowExt};

    use crate::{config::SettingsManager, ui::preferences::utils::create_combo_row_from_settings};

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_create_combo_row_from_settings() {
        // Create temporary settings file
        let temp_file = PathBuf::from("/tmp/oxhidifi_test_combo_row.json");
        let _ = remove_file(&temp_file);

        let settings_manager = SettingsManager::with_config_path(temp_file.clone()).unwrap();
        let settings_manager_arc = Arc::new(settings_manager);

        let options = vec![
            "Option1".to_string(),
            "Option2".to_string(),
            "Option3".to_string(),
        ];
        let current_value = "Option2".to_string();

        let combo_row = create_combo_row_from_settings(
            "Test Title",
            Some("Test Subtitle"),
            options,
            current_value,
            |_settings| "grid".to_string(), // Default value since setting is removed
            |_settings, _value| {},         // No-op since setting is removed
            settings_manager_arc.clone(),
        );

        // Verify the combo row was created with correct properties
        assert_eq!(combo_row.title(), "Test Title");
        assert_eq!(combo_row.subtitle().as_deref(), Some("Test Subtitle"));

        // Clean up
        let _ = remove_file(temp_file);
    }
}

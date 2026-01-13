//! Tests for preferences dialog functionality.
//!
//! This module provides unit and integration tests for the preferences
//! dialog components to ensure proper functionality and persistence.

use std::{fs::remove_file, path::PathBuf, sync::Arc};

use {parking_lot::RwLock, tempfile::TempDir};

use crate::{
    audio::engine::AudioEngine,
    config::{SettingsManager, UserSettings},
    state::{AppState, app_state::LibraryTab},
    ui::preferences::{
        AudioPreferencesPage, GeneralPreferencesPage, LibraryPreferencesPage, PreferencesDialog,
    },
};

#[test]
fn test_preferences_dialog_creation() {
    // Create temporary settings file
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone()).unwrap();
    let settings_manager_arc = Arc::new(settings_manager);

    // Create mock AppState
    let engine = AudioEngine::new().unwrap();
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    // Create preferences dialog
    let dialog = PreferencesDialog::new(&app_state_arc, &settings_manager_arc);

    // Verify dialog was created successfully
    assert!(dialog.widget.title().is_none());
}

#[test]
fn test_general_preferences_page_creation() {
    // Create temporary settings file
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone()).unwrap();
    let settings_manager_arc = Arc::new(settings_manager);

    // Create mock AppState
    let engine = AudioEngine::new().unwrap();
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    // Create general preferences page
    let page = GeneralPreferencesPage::new(app_state_arc, settings_manager_arc);

    // Verify page was created successfully
    assert_eq!(page.widget.title(), "General");
}

#[test]
fn test_library_preferences_page_creation() {
    // Create temporary settings file
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone()).unwrap();
    let settings_manager_arc = Arc::new(settings_manager);

    // Create mock AppState
    let engine = AudioEngine::new().unwrap();
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    // Create library preferences page
    let page = LibraryPreferencesPage::new(app_state_arc, settings_manager_arc);

    // Verify page was created successfully
    assert_eq!(page.widget.title(), "Library");
}

#[test]
fn test_audio_preferences_page_creation() {
    // Create temporary settings file
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone()).unwrap();
    let settings_manager_arc = Arc::new(settings_manager);

    // Create mock AppState
    let engine = AudioEngine::new().unwrap();
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    // Create audio preferences page
    let page = AudioPreferencesPage::new(app_state_arc, settings_manager_arc);

    // Verify page was created successfully
    assert_eq!(page.widget.title(), "Audio");
}

#[test]
fn test_settings_persistence_across_sessions() {
    // Create temporary settings file
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    // First session: Create settings with non-default values
    let mut settings_manager = SettingsManager::with_config_path(settings_path.clone()).unwrap();
    let mut current_settings = settings_manager.get_settings().clone();
    current_settings.theme_preference = "dark".to_string();
    current_settings.show_dr_values = false;
    current_settings.library_directories = vec!["/music/test".to_string()];
    current_settings.sample_rate = 96000;
    current_settings.exclusive_mode = false;
    current_settings.buffer_duration_ms = 100;
    settings_manager.update_settings(current_settings).unwrap();

    // Create preferences components and verify initial values
    let settings_manager_arc = Arc::new(settings_manager);

    // Create mock AppState
    let engine = AudioEngine::new().unwrap();
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    // Create general preferences page and verify settings
    let general_page =
        GeneralPreferencesPage::new(app_state_arc.clone(), settings_manager_arc.clone());
    let current_theme = settings_manager_arc.get_settings().theme_preference.clone();
    assert_eq!(current_theme, "dark");

    let current_show_dr = settings_manager_arc.get_settings().show_dr_values;
    assert_eq!(current_show_dr, false);

    // Create library preferences page and verify settings
    let library_page =
        LibraryPreferencesPage::new(app_state_arc.clone(), settings_manager_arc.clone());
    let current_directories = settings_manager_arc
        .get_settings()
        .library_directories
        .clone();
    assert_eq!(current_directories, vec!["/music/test".to_string()]);

    // Create audio preferences page and verify settings
    let audio_page = AudioPreferencesPage::new(app_state_arc.clone(), settings_manager_arc.clone());
    let current_sample_rate = settings_manager_arc.get_settings().sample_rate;
    assert_eq!(current_sample_rate, 96000);

    let current_exclusive_mode = settings_manager_arc.get_settings().exclusive_mode;
    assert_eq!(current_exclusive_mode, false);

    let current_buffer_duration = settings_manager_arc.get_settings().buffer_duration_ms;
    assert_eq!(current_buffer_duration, 100);

    // Second session: Create new settings manager and verify persistence
    let settings_manager2 = SettingsManager::with_config_path(settings_path.clone()).unwrap();
    let settings_manager2_arc = Arc::new(settings_manager2);

    assert_eq!(
        settings_manager2_arc.get_settings().theme_preference,
        "dark"
    );
    assert_eq!(settings_manager2_arc.get_settings().show_dr_values, false);
    assert_eq!(
        settings_manager2_arc.get_settings().library_directories,
        vec!["/music/test".to_string()]
    );
    assert_eq!(settings_manager2_arc.get_settings().sample_rate, 96000);
    assert_eq!(settings_manager2_arc.get_settings().exclusive_mode, false);
    assert_eq!(settings_manager2_arc.get_settings().buffer_duration_ms, 100);

    // Clean up
    remove_file(settings_path).ok();
}

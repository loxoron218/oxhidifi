//! Tests for preferences dialog functionality.
//!
//! This module provides unit and integration tests for the preferences
//! dialog components to ensure proper functionality and persistence.

use std::{fs::remove_file, path::PathBuf, sync::Arc};

use {
    anyhow::{Result, bail},
    parking_lot::RwLock,
    tempfile::TempDir,
};

use crate::{
    audio::engine::AudioEngine,
    config::{SettingsManager, UserSettings},
    library::database::LibraryDatabase,
    state::app_state::{AppState, LibraryTab},
    ui::preferences::{
        audio_page::AudioPreferencesPage, dialog::PreferencesDialog,
        general_page::GeneralPreferencesPage, library_page::LibraryPreferencesPage,
    },
};

#[test]
fn test_preferences_dialog_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone())?;
    let settings_manager_arc = Arc::new(settings_manager);

    let engine = AudioEngine::new()?;
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    let dialog = PreferencesDialog::new(&app_state_arc);

    assert!(dialog.widget.title().is_none());
    Ok(())
}

#[test]
fn test_general_preferences_page_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone())?;
    let settings_manager_arc = Arc::new(settings_manager);

    let engine = AudioEngine::new()?;
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    let page = GeneralPreferencesPage::new(app_state_arc, settings_manager_arc);

    assert_eq!(page.widget.title(), "General");
    Ok(())
}

#[test]
fn test_library_preferences_page_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone())?;
    let settings_manager_arc = Arc::new(settings_manager);

    let engine = AudioEngine::new()?;
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    let library_db = LibraryDatabase::new()?;
    let page =
        LibraryPreferencesPage::new(app_state_arc, Arc::new(library_db), settings_manager_arc);

    assert_eq!(page.widget.title(), "Library");
    Ok(())
}

#[test]
fn test_audio_preferences_page_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let settings_path = temp_dir.path().join("settings.json");

    let settings_manager = SettingsManager::with_config_path(settings_path.clone())?;
    let settings_manager_arc = Arc::new(settings_manager);

    let engine = AudioEngine::new()?;
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    let page = AudioPreferencesPage::new(settings_manager_arc);

    assert_eq!(page.widget.title(), "Audio");
    Ok(())
}

#[test]
fn test_settings_persistence_across_sessions() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let settings_path = temp_dir.path().join("settings.json");

    let mut settings_manager = SettingsManager::with_config_path(settings_path.clone())?;
    let mut current_settings = settings_manager.get_settings().clone();
    current_settings.theme_preference = "dark".to_string();
    current_settings.show_dr_values = false;
    current_settings.library_directories = vec!["/music/test".to_string()];
    current_settings.sample_rate = 96000;
    current_settings.exclusive_mode = false;
    current_settings.buffer_duration_ms = 100;
    settings_manager.update_settings(current_settings)?;

    let settings_manager_arc = Arc::new(settings_manager);

    let engine = AudioEngine::new()?;
    let engine_weak = Arc::downgrade(&Arc::new(engine));
    let app_state = AppState::new(
        engine_weak,
        None,
        Arc::new(RwLock::new(settings_manager_arc.clone())),
    );
    let app_state_arc = Arc::new(app_state);

    let _general_page =
        GeneralPreferencesPage::new(app_state_arc.clone(), settings_manager_arc.clone());
    let current_theme = settings_manager_arc.get_settings().theme_preference.clone();
    assert_eq!(current_theme, "dark");

    let current_show_dr = settings_manager_arc.get_settings().show_dr_values;
    assert!(!current_show_dr);

    let library_db2 = LibraryDatabase::new()?;
    let _library_page = LibraryPreferencesPage::new(
        app_state_arc.clone(),
        Arc::new(library_db2),
        settings_manager_arc.clone(),
    );
    let current_directories = settings_manager_arc
        .get_settings()
        .library_directories
        .clone();
    assert_eq!(current_directories, vec!["/music/test".to_string()]);

    let _audio_page = AudioPreferencesPage::new(settings_manager_arc.clone());
    let current_sample_rate = settings_manager_arc.get_settings().sample_rate;
    assert_eq!(current_sample_rate, 96000);

    let current_exclusive_mode = settings_manager_arc.get_settings().exclusive_mode;
    assert!(!current_exclusive_mode);

    let current_buffer_duration = settings_manager_arc.get_settings().buffer_duration_ms;
    assert_eq!(current_buffer_duration, 100);

    let settings_manager2 = SettingsManager::with_config_path(settings_path.clone())?;
    let settings_manager2_arc = Arc::new(settings_manager2);

    assert_eq!(
        settings_manager2_arc.get_settings().theme_preference,
        "dark"
    );
    assert!(!settings_manager2_arc.get_settings().show_dr_values);
    assert_eq!(
        settings_manager2_arc.get_settings().library_directories,
        vec!["/music/test".to_string()]
    );
    assert_eq!(settings_manager2_arc.get_settings().sample_rate, 96000);
    assert!(!settings_manager2_arc.get_settings().exclusive_mode);
    assert_eq!(settings_manager2_arc.get_settings().buffer_duration_ms, 100);

    remove_file(settings_path).ok();
    Ok(())
}

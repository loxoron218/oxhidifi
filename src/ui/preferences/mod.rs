//! Preferences dialog implementation following GNOME HIG guidelines.
//!
//! This module provides the preferences dialog with three tabs:
//! General, Library, and Audio settings management.

pub mod audio_page;
pub mod dialog;
pub mod general_page;
pub mod library_page;
pub mod utils;

pub use {
    audio_page::AudioPreferencesPage, dialog::PreferencesDialog,
    general_page::GeneralPreferencesPage, library_page::LibraryPreferencesPage,
};

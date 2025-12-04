//! User preferences, settings, and persistent state management.
//!
//! This module provides user preference management with XDG Base Directory
//! compliance and persistent state handling.

pub mod settings;

pub use settings::{SettingsError, SettingsManager, UserSettings, get_cache_dir, get_config_path};

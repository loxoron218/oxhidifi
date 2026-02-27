//! Zoom level management with persistence and reactive updates.
//!
//! This module provides the `ZoomManager` which handles zoom level state
//! for both grid and list views, with automatic persistence to settings
//! and reactive notifications to UI components.

use std::sync::Arc;

use {
    async_channel::{Receiver, Sender, unbounded},
    parking_lot::RwLock,
    tracing::debug,
};

use crate::config::settings::SettingsManager;

/// Zoom level change events.
#[derive(Debug, Clone)]
pub enum ZoomEvent {
    /// Grid view zoom level changed.
    GridZoomChanged(u8),
    /// List view zoom level changed.
    ListZoomChanged(u8),
}

/// Manages zoom levels for different view modes with persistence.
///
/// The `ZoomManager` provides thread-safe access to zoom levels and
/// automatically persists changes to the user settings.
#[derive(Debug, Clone)]
pub struct ZoomManager {
    /// Current grid view zoom level (0-4).
    pub grid_zoom_level: Arc<RwLock<u8>>,
    /// Current list view zoom level (0-2).
    pub list_zoom_level: Arc<RwLock<u8>>,
    /// Settings manager reference for persistence.
    pub settings_manager: Arc<RwLock<SettingsManager>>,
    /// List of active subscribers for manual broadcast fan-out.
    subscribers: Arc<RwLock<Vec<Sender<ZoomEvent>>>>,
}

impl ZoomManager {
    /// Creates a new zoom manager instance.
    ///
    /// # Arguments
    ///
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `ZoomManager` instance.
    pub fn new(settings_manager: Arc<RwLock<SettingsManager>>) -> Self {
        let (grid_zoom_level, list_zoom_level) = {
            let settings_guard = settings_manager.read();
            let (grid, list) = {
                let settings = settings_guard.get_settings();
                (settings.grid_zoom_level, settings.list_zoom_level)
            };
            drop(settings_guard);
            (grid, list)
        };
        debug!(
            "ZoomManager: Loaded initial zoom levels from settings - grid: {}, list: {}",
            grid_zoom_level, list_zoom_level
        );

        Self {
            grid_zoom_level: Arc::new(RwLock::new(grid_zoom_level)),
            list_zoom_level: Arc::new(RwLock::new(list_zoom_level)),
            settings_manager,
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Helper to broadcast an event to all subscribers.
    /// Cleans up closed channels.
    fn broadcast_event(&self, event: &ZoomEvent) -> usize {
        let mut subscribers = self.subscribers.write();
        let mut active = Vec::with_capacity(subscribers.len());
        let mut count = 0;

        for tx in subscribers.iter() {
            if matches!(tx.try_send(event.clone()), Ok(())) {
                active.push(tx.clone());
                count += 1;
            }
        }

        *subscribers = active;
        count
    }

    /// Gets the current grid view zoom level.
    ///
    /// # Returns
    ///
    /// The current grid zoom level (0-4).
    #[must_use]
    pub fn get_grid_zoom_level(&self) -> u8 {
        *self.grid_zoom_level.read()
    }

    /// Gets the current list view zoom level.
    ///
    /// # Returns
    ///
    /// The current list zoom level (0-2).
    #[must_use]
    pub fn get_list_zoom_level(&self) -> u8 {
        *self.list_zoom_level.read()
    }

    /// Sets the grid view zoom level and persists to settings.
    ///
    /// # Arguments
    ///
    /// * `level` - New zoom level (0-4)
    pub fn set_grid_zoom_level(&self, level: u8) {
        let clamped_level = level.min(4); // Clamp to valid range 0-4

        if *self.grid_zoom_level.read() != clamped_level {
            debug!("ZoomManager: Setting grid zoom level to {}", clamped_level);
            *self.grid_zoom_level.write() = clamped_level;

            // Persist to settings
            let settings_manager_write = self.settings_manager.write();
            let mut current_settings = settings_manager_write.get_settings().clone();
            current_settings.grid_zoom_level = clamped_level;
            let config_path = settings_manager_write.get_config_path();
            debug!(
                "Persisting grid zoom level {} to config file: {:?}",
                clamped_level, config_path
            );
            if let Err(e) = settings_manager_write.update_settings(current_settings) {
                debug!("Failed to persist grid zoom level {}: {}", clamped_level, e);
            }
            drop(settings_manager_write);

            self.broadcast_event(&ZoomEvent::GridZoomChanged(clamped_level));
        }
    }

    /// Sets the list view zoom level and persists to settings.
    ///
    /// # Arguments
    ///
    /// * `level` - New zoom level (0-2)
    pub fn set_list_zoom_level(&self, level: u8) {
        let clamped_level = level.min(2); // Clamp to valid range 0-2

        if *self.list_zoom_level.read() != clamped_level {
            debug!("ZoomManager: Setting list zoom level to {}", clamped_level);
            *self.list_zoom_level.write() = clamped_level;

            // Persist to settings
            let settings_manager_write = self.settings_manager.write();
            let mut current_settings = settings_manager_write.get_settings().clone();
            current_settings.list_zoom_level = clamped_level;
            let config_path = settings_manager_write.get_config_path();
            debug!(
                "Persisting list zoom level {} to config file: {:?}",
                clamped_level, config_path
            );
            if let Err(e) = settings_manager_write.update_settings(current_settings) {
                debug!("Failed to persist list zoom level {}: {}", clamped_level, e);
            }
            drop(settings_manager_write);

            self.broadcast_event(&ZoomEvent::ListZoomChanged(clamped_level));
        }
    }

    /// Subscribes to zoom level changes.
    ///
    /// # Returns
    ///
    /// A receiver for zoom change events.
    pub fn subscribe(&self) -> Receiver<ZoomEvent> {
        debug!("ZoomManager: New subscription created");

        let (tx, rx) = unbounded();
        self.subscribers.write().push(tx);

        rx
    }

    /// Gets the cover art dimensions for grid view based on current zoom level.
    ///
    /// # Returns
    ///
    /// (width, height) tuple for cover art dimensions.
    #[must_use]
    pub fn get_grid_cover_dimensions(&self) -> (i32, i32) {
        let zoom_level = self.get_grid_zoom_level();
        match zoom_level {
            0 => (120, 120), // Smallest
            1 => (150, 150), // Small
            3 => (210, 210), // Large
            4 => (240, 240), // Largest
            _ => (180, 180), // Fallback to default (Medium)
        }
    }

    /// Gets the cover art dimensions for list view based on current zoom level.
    ///
    /// # Returns
    ///
    /// (width, height) tuple for cover art dimensions.
    #[must_use]
    pub fn get_list_cover_dimensions(&self) -> (i32, i32) {
        let zoom_level = self.get_list_zoom_level();
        match zoom_level {
            0 => (32, 32), // Smallest
            2 => (64, 64), // Largest
            _ => (48, 48), // Fallback to default (Medium)
        }
    }

    /// Gets the row height for list view based on current zoom level.
    ///
    /// # Returns
    ///
    /// Row height in pixels.
    #[must_use]
    pub fn get_list_row_height(&self) -> i32 {
        let zoom_level = self.get_list_zoom_level();
        match zoom_level {
            0 => 60,  // Smallest
            2 => 100, // Largest
            _ => 80,  // Fallback to default (Medium)
        }
    }

    /// Gets the minimum width for album tiles in grid view based on current zoom level.
    ///
    /// # Returns
    ///
    /// Minimum width in pixels.
    #[must_use]
    pub fn get_grid_min_width(&self) -> i32 {
        let zoom_level = self.get_grid_zoom_level();
        match zoom_level {
            0 => 120, // Smallest
            1 => 150, // Small
            3 => 210, // Large
            4 => 240, // Largest
            _ => 180, // Fallback to default (Medium)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::remove_file, path::PathBuf, sync::Arc};

    use {
        anyhow::{Result, bail},
        parking_lot::RwLock,
        tempfile::TempDir,
        tokio::test as TokioTest,
        tracing::debug,
    };

    use crate::{config::settings::SettingsManager, state::zoom_manager::ZoomManager};

    #[test]
    fn test_zoom_manager_creation() -> Result<()> {
        // Use a non-existent file path to ensure default settings are used
        let temp_file = PathBuf::from("/tmp/oxhidifi_test_settings_1.json");

        // Remove file if it exists to ensure clean state
        if let Err(e) = remove_file(&temp_file) {
            debug!(
                "Test cleanup: Failed to remove file '{}': {}",
                temp_file.display(),
                e
            );
        }

        let settings_manager = SettingsManager::with_config_path(temp_file)?;
        let settings_manager_arc = Arc::new(RwLock::new(settings_manager));
        let zoom_manager = ZoomManager::new(settings_manager_arc);

        if zoom_manager.get_grid_zoom_level() != 2 {
            bail!("Expected grid zoom level to be 2");
        }
        if zoom_manager.get_list_zoom_level() != 1 {
            bail!("Expected list zoom level to be 1");
        }
        Ok(())
    }

    #[test]
    fn test_grid_zoom_levels() -> Result<()> {
        // Use a non-existent file path to ensure default settings are used
        let temp_file = PathBuf::from("/tmp/oxhidifi_test_settings_2.json");

        // Remove file if it exists to ensure clean state
        if let Err(e) = remove_file(&temp_file) {
            debug!(
                "Test cleanup: Failed to remove file '{}': {}",
                temp_file.display(),
                e
            );
        }

        let settings_manager = SettingsManager::with_config_path(temp_file)?;
        let settings_manager_arc = Arc::new(RwLock::new(settings_manager));
        let zoom_manager = ZoomManager::new(settings_manager_arc);

        // Test all valid grid zoom levels
        for level in 0..=4 {
            zoom_manager.set_grid_zoom_level(level);
            if zoom_manager.get_grid_zoom_level() != level {
                bail!("Expected grid zoom level to be {level}");
            }

            let (width, height) = zoom_manager.get_grid_cover_dimensions();
            if width != height {
                bail!("Expected grid cover dimensions to be square");
            }
            if !(120..=240).contains(&width) {
                bail!("Grid cover width {width} out of expected range 120-240");
            }
        }

        // Test clamping
        zoom_manager.set_grid_zoom_level(10);
        if zoom_manager.get_grid_zoom_level() != 4 {
            bail!("Expected grid zoom level to be clamped to 4");
        }
        Ok(())
    }

    #[test]
    fn test_list_zoom_levels() -> Result<()> {
        // Use a non-existent file path to ensure default settings are used
        let temp_file = PathBuf::from("/tmp/oxhidifi_test_settings_3.json");

        // Remove file if it exists to ensure clean state
        if let Err(e) = remove_file(&temp_file) {
            debug!(
                "Test cleanup: Failed to remove file '{}': {}",
                temp_file.display(),
                e
            );
        }

        let settings_manager = SettingsManager::with_config_path(temp_file)?;
        let settings_manager_arc = Arc::new(RwLock::new(settings_manager));
        let zoom_manager = ZoomManager::new(settings_manager_arc);

        // Test all valid list zoom levels
        for level in 0..=2 {
            zoom_manager.set_list_zoom_level(level);
            if zoom_manager.get_list_zoom_level() != level {
                bail!("Expected list zoom level to be {level}");
            }

            let (width, height) = zoom_manager.get_list_cover_dimensions();
            if width != height {
                bail!("Expected list cover dimensions to be square");
            }
            if !(32..=64).contains(&width) {
                bail!("List cover width {width} out of expected range 32-64");
            }

            let row_height = zoom_manager.get_list_row_height();
            if !(60..=100).contains(&row_height) {
                bail!("List row height {row_height} out of expected range 60-100");
            }
        }

        // Test clamping
        zoom_manager.set_list_zoom_level(10);
        if zoom_manager.get_list_zoom_level() != 2 {
            bail!("Expected list zoom level to be clamped to 2");
        }
        Ok(())
    }

    #[TokioTest]
    async fn test_zoom_persistence_across_sessions() -> Result<()> {
        // Create a temporary directory for our test
        let temp_dir = TempDir::new()?;
        let settings_path = temp_dir.path().join("settings.json");

        // First session: Create settings with non-default zoom levels
        let initial_grid_level = 3;
        let initial_list_level = 0;

        let settings_manager = SettingsManager::with_config_path(settings_path.clone())?;
        let mut current_settings = settings_manager.get_settings().clone();
        current_settings.grid_zoom_level = initial_grid_level;
        current_settings.list_zoom_level = initial_list_level;
        settings_manager.update_settings(current_settings)?;

        // Create zoom manager and verify initial zoom levels
        let settings_manager_arc = Arc::new(RwLock::new(settings_manager));
        let zoom_manager = ZoomManager::new(settings_manager_arc);

        // Verify that zoom levels were loaded correctly from settings
        if zoom_manager.get_grid_zoom_level() != initial_grid_level {
            bail!("Expected grid zoom level to be {initial_grid_level}");
        }
        if zoom_manager.get_list_zoom_level() != initial_list_level {
            bail!("Expected list zoom level to be {initial_list_level}");
        }

        // Change zoom levels
        zoom_manager.set_grid_zoom_level(1);
        zoom_manager.set_list_zoom_level(2);

        // Verify changes are reflected immediately
        if zoom_manager.get_grid_zoom_level() != 1 {
            bail!("Expected grid zoom level to be 1");
        }
        if zoom_manager.get_list_zoom_level() != 2 {
            bail!("Expected list zoom level to be 2");
        }

        // Second session: Create new zoom manager and verify persistence
        let settings_manager2 = SettingsManager::with_config_path(settings_path.clone())?;
        let settings_manager2_arc = Arc::new(RwLock::new(settings_manager2));
        let zoom_manager2 = ZoomManager::new(settings_manager2_arc);

        // Verify that zoom levels were restored from persisted settings
        if zoom_manager2.get_grid_zoom_level() != 1 {
            bail!("Expected persisted grid zoom level to be 1");
        }
        if zoom_manager2.get_list_zoom_level() != 2 {
            bail!("Expected persisted list zoom level to be 2");
        }

        // Clean up (tempdir will be automatically cleaned up)
        if let Err(e) = remove_file(&settings_path) {
            debug!(
                "Test cleanup: Failed to remove file '{}': {}",
                settings_path.display(),
                e
            );
        }
        Ok(())
    }
}

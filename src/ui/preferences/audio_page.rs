//! Audio preferences page implementation.
//!
//! This module implements the Audio preferences tab which handles
//! audio output configuration including device selection, sample rate,
//! exclusive mode, and buffer duration settings.

use std::sync::Arc;

use {
    libadwaita::{
        ActionRow, PreferencesGroup, PreferencesPage, SpinRow, SwitchRow,
        gtk::{AccessibleRole::Group, Adjustment},
        prelude::{ActionRowExt, PreferencesGroupExt, PreferencesPageExt},
    },
    tracing::{debug, error},
};

use crate::{config::SettingsManager, state::AppState};

/// Audio preferences page with output configuration settings.
pub struct AudioPreferencesPage {
    /// The underlying Libadwaita preferences page widget.
    pub widget: PreferencesPage,
    /// Settings manager reference for persistence.
    settings_manager: Arc<SettingsManager>,
}

impl AudioPreferencesPage {
    /// Creates a new audio preferences page instance.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `AudioPreferencesPage` instance.
    pub fn new(_app_state: Arc<AppState>, settings_manager: Arc<SettingsManager>) -> Self {
        let widget = PreferencesPage::builder()
            .title("Audio")
            .icon_name("audio-speakers-symbolic")
            .accessible_role(Group)
            .build();

        let mut page = Self {
            widget,
            settings_manager,
        };

        page.setup_audio_output_group();
        page.setup_playback_group();

        debug!("AudioPreferencesPage: Created");

        page
    }

    /// Sets up the audio output configuration group.
    fn setup_audio_output_group(&mut self) {
        let group = PreferencesGroup::builder()
            .title("Audio Output")
            .description("Configure audio playback device and format")
            .build();

        // Audio device selection (simplified - in practice would enumerate CPAL devices)
        let current_device = self.settings_manager.get_settings().audio_device.clone();

        let device_row = ActionRow::builder()
            .title("Output Device")
            .subtitle("Select audio output device for playback")
            .build();

        let device_subtitle = if let Some(ref device) = current_device {
            device.clone()
        } else {
            "System Default".to_string()
        };
        device_row.set_subtitle(&device_subtitle);

        // In a complete implementation, this would open a device selection dialog
        // For now, we'll just show the current device
        let _settings_manager_clone = self.settings_manager.clone();
        device_row.connect_activated(move |_| {
            debug!("AudioPreferencesPage: Device selection dialog would be shown here");

            // TODO: Implement actual device enumeration and selection
        });

        group.add(&device_row);

        // Sample rate configuration
        self.setup_sample_rate_preference(&group);

        // Exclusive mode toggle
        self.setup_exclusive_mode_preference(&group);

        self.widget.add(&group);
    }

    /// Sets up the sample rate preference spin row.
    fn setup_sample_rate_preference(&self, group: &PreferencesGroup) {
        let current_sample_rate = self.settings_manager.get_settings().sample_rate;

        // Create adjustment for sample rate (0 = auto, then common rates)
        let adjustment = Adjustment::new(
            current_sample_rate as f64,
            0.0,      // minimum (0 = auto)
            768000.0, // maximum (768kHz)
            1.0,      // step
            1000.0,   // page increment
            0.0,      // page size
        );

        let spin_row = SpinRow::builder()
            .title("Sample Rate")
            .subtitle("Set output sample rate (0 = auto-detect)")
            .adjustment(&adjustment)
            .numeric(true)
            .build();

        // Connect change handler
        let settings_manager_clone = self.settings_manager.clone();
        spin_row.connect_value_notify(move |row| {
            let new_value = row.value() as u32;

            // Validate sample rate (common rates or 0 for auto)
            let valid_rates = [
                0, 8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 88200, 96000, 176400,
                192000, 352800, 384000, 705600, 768000,
            ];

            if !valid_rates.contains(&new_value) {
                debug!("Invalid sample rate: {}, reverting to auto", new_value);

                // Revert to auto if invalid
                row.set_value(0.0);
                return;
            }

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.sample_rate = new_value;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                error!("Failed to update sample rate preference: {}", e);
            }
        });

        group.add(&spin_row);
    }

    /// Sets up the exclusive mode preference switch row.
    fn setup_exclusive_mode_preference(&self, group: &PreferencesGroup) {
        let current_exclusive_mode = self.settings_manager.get_settings().exclusive_mode;

        let switch_row = SwitchRow::builder()
            .title("Exclusive Mode")
            .subtitle(
                "Use exclusive mode for bit-perfect audio playback (may not work with all devices)",
            )
            .active(current_exclusive_mode)
            .build();

        // Connect change handler
        let settings_manager_clone = self.settings_manager.clone();
        switch_row.connect_active_notify(move |row| {
            let new_value = row.is_active();

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.exclusive_mode = new_value;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                error!("Failed to update exclusive mode preference: {}", e);
            }
        });

        group.add(&switch_row);
    }

    /// Sets up the playback configuration group.
    fn setup_playback_group(&mut self) {
        let group = PreferencesGroup::builder()
            .title("Playback")
            .description("Configure playback buffer and performance settings")
            .build();

        // Buffer duration configuration
        self.setup_buffer_duration_preference(&group);

        self.widget.add(&group);
    }

    /// Sets up the buffer duration preference spin row.
    fn setup_buffer_duration_preference(&self, group: &PreferencesGroup) {
        let current_buffer_duration = self.settings_manager.get_settings().buffer_duration_ms;

        // Create adjustment for buffer duration (10ms to 500ms)
        let adjustment = Adjustment::new(
            current_buffer_duration as f64,
            10.0,  // minimum
            500.0, // maximum
            1.0,   // step
            10.0,  // page increment
            0.0,   // page size
        );

        let spin_row = SpinRow::builder()
            .title("Buffer Duration")
            .subtitle("Audio buffer duration in milliseconds (lower = less latency, higher = more stable)")
            .adjustment(&adjustment)
            .numeric(true)
            .build();

        // Connect change handler
        let settings_manager_clone = self.settings_manager.clone();
        spin_row.connect_value_notify(move |row| {
            let new_value = row.value() as u32;

            // Validate buffer duration (reasonable range: 10-500ms)
            if !(10..=500).contains(&new_value) {
                debug!(
                    "Invalid buffer duration: {}, clamping to valid range",
                    new_value
                );
                let clamped_value = new_value.clamp(10, 500);
                row.set_value(clamped_value as f64);
                return;
            }

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.buffer_duration_ms = new_value;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                error!("Failed to update buffer duration preference: {}", e);
            }
        });

        group.add(&spin_row);
    }
}

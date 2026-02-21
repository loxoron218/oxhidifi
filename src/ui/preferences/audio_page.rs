//! Audio preferences page implementation.
//!
//! This module implements the Audio preferences tab which handles
//! audio output configuration including device selection, sample rate,
//! and buffer duration settings.

use std::sync::Arc;

use {
    libadwaita::{
        ActionRow, PreferencesGroup, PreferencesPage, SpinRow,
        gtk::{AccessibleRole::Group, Adjustment},
        prelude::{ActionRowExt, PreferencesGroupExt, PreferencesPageExt},
    },
    num_traits::cast::ToPrimitive,
    tracing::{debug, error, info},
};

use crate::config::SettingsManager;

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
    /// * `settings_manager` - Settings manager reference for persistence
    ///
    /// # Returns
    ///
    /// A new `AudioPreferencesPage` instance.
    pub fn new(settings_manager: Arc<SettingsManager>) -> Self {
        let widget = PreferencesPage::builder()
            .title("Audio")
            .icon_name("audio-speakers-symbolic")
            .accessible_role(Group)
            .build();

        let page = Self {
            widget,
            settings_manager,
        };

        page.setup_audio_output_group();
        page.setup_playback_group();

        debug!("AudioPreferencesPage: Created");

        page
    }

    /// Sets up the audio output configuration group.
    fn setup_audio_output_group(&self) {
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

        let device_subtitle = current_device
            .as_ref()
            .map_or_else(|| "System Default".to_string(), Clone::clone);
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

        self.widget.add(&group);
    }

    /// Sets up the sample rate preference spin row.
    fn setup_sample_rate_preference(&self, group: &PreferencesGroup) {
        let current_sample_rate = self.settings_manager.get_settings().sample_rate;

        // Create adjustment for sample rate (0 = auto, then common rates)
        let adjustment = Adjustment::new(
            f64::from(current_sample_rate),
            0.0,       // minimum (0 = auto)
            768_000.0, // maximum (768kHz)
            1.0,       // step
            1_000.0,   // page increment
            0.0,       // page size
        );

        let spin_row = SpinRow::builder()
            .title("Sample Rate")
            .subtitle("Set output sample rate (0 = auto-detect)")
            .adjustment(&adjustment)
            .numeric(true)
            .build();

        // Connect change handler
        let settings_manager_clone = self.settings_manager.clone();
        spin_row.connect_value_notify(move |row: &SpinRow| {
            let clamped_value = row.value().clamp(0.0_f64, f64::MAX);
            let Some(i64_value) = clamped_value.to_i64() else {
                info!("Invalid sample rate value: cannot convert to i64");
                return;
            };
            let Ok(new_value) = u32::try_from(i64_value) else {
                info!("Invalid sample rate value: exceeds u32 range");
                return;
            };

            // Validate sample rate (common rates or 0 for auto)
            let valid_rates = [
                0, 8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 88_200, 96_000,
                176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
            ];

            if !valid_rates.contains(&new_value) {
                info!("Invalid sample rate: {}, reverting to auto", new_value);

                // Revert to auto if invalid
                row.set_value(0.0);
                return;
            }

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.sample_rate = new_value;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                error!(error = %e, "Failed to update sample rate preference");
            }
        });

        group.add(&spin_row);
    }

    /// Sets up the playback configuration group.
    fn setup_playback_group(&self) {
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
            f64::from(current_buffer_duration),
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
        spin_row.connect_value_notify(move |row: &SpinRow| {
            let clamped_value = row.value().clamp(0.0_f64, f64::MAX);
            let Some(i64_value) = clamped_value.to_i64() else {
                info!("Invalid buffer duration value: cannot convert to i64");
                return;
            };
            let Ok(new_value) = u32::try_from(i64_value) else {
                info!("Invalid buffer duration value: exceeds u32 range");
                return;
            };

            // Validate buffer duration (reasonable range: 10-500ms)
            if !(10..=500).contains(&new_value) {
                info!(
                    "Invalid buffer duration: {}, clamping to valid range",
                    new_value
                );
                let clamped_value = new_value.clamp(10, 500);
                row.set_value(f64::from(clamped_value));
                return;
            }

            // Update settings
            let mut current_settings = settings_manager_clone.get_settings().clone();
            current_settings.buffer_duration_ms = new_value;

            if let Err(e) = settings_manager_clone.update_settings(current_settings) {
                error!(error = %e, "Failed to update buffer duration preference");
            }
        });

        group.add(&spin_row);
    }
}

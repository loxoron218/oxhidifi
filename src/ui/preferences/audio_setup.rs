//! Audio output and playback preferences pages.

use std::sync::Arc;

use {
    libadwaita::{
        ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage, SpinRow, SwitchRow,
        gio::spawn_blocking,
        glib::spawn_future_local,
        gtk::{Adjustment, StringList},
        prelude::{
            ActionRowExt, ComboRowExt, ObjectExt, PreferencesDialogExt, PreferencesGroupExt,
            PreferencesPageExt, PreferencesRowExt,
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::runtime::AppState,
    playback::{
        devices::{
            DeviceInfo,
            OutputMode::{self, BitPerfect, Resampled},
            list_output_devices,
        },
        transport::PlaybackTransport,
    },
    storage::database::SqliteStorage,
};

/// Save the audio device selection for the given combo index.
async fn set_audio_device(state: &Arc<AppState>, idx: u32) {
    let Ok(Ok(devices)) = spawn_blocking(list_output_devices).await else {
        return;
    };
    let name = devices
        .get(usize::try_from(idx).unwrap_or(0))
        .map(|d| d.name.clone());
    if state.storage.get_audio_device().as_deref() == name.as_deref() {
        return;
    }
    info!(
        audio_device = name.as_deref().unwrap_or("default"),
        "Audio device selection changed",
    );
    if let Err(e) = state.storage.set_audio_device(name).await {
        error!(error = %e, "Failed to save audio device selection");
    }
}

/// Select the preferred audio device in the combo if it exists in the list.
fn set_preferred_device(combo: &ComboRow, devices: &[DeviceInfo], preferred: Option<&String>) {
    if let Some(pref) = preferred
        && let Some(i) = devices.iter().position(|d| &d.name == pref)
    {
        combo.set_selected(u32::try_from(i).unwrap_or(0));
    }
}

/// Persist gapless playback setting, logging on failure.
async fn save_gapless_setting(state: Arc<AppState>, enabled: bool) {
    if let Err(e) = state.storage.set_gapless_enabled(enabled).await {
        error!(error = %e, "Failed to save gapless setting");
    }
}

/// Persist output mode, logging on failure.
async fn persist_output_mode(storage: Arc<SqliteStorage>, mode: OutputMode) {
    if let Err(e) = storage.set_output_mode(mode).await {
        warn!(error = %e, "Failed to persist output mode");
    }
}

/// Build the Audio > Output and Audio > Playback group.
pub fn build_audio_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
    let page = PreferencesPage::new();
    page.set_title("Audio");
    page.set_icon_name(Some("audio-speakers-symbolic"));

    let output_group = PreferencesGroup::new();
    output_group.set_title("Output");
    output_group.set_description(Some("Audio output device"));

    let device_combo = ComboRow::new();
    device_combo.set_title("Audio Device");

    let state_devices = Arc::clone(state);
    let combo = device_combo.clone();
    spawn_future_local(async move {
        let devices = match spawn_blocking(list_output_devices).await {
            Ok(Ok(devices)) => devices,
            Ok(Err(e)) => {
                warn!(error = %e, "Failed to enumerate audio devices");
                return;
            }
            Err(e) => {
                warn!(error = ?e, "Failed to enumerate audio devices");
                return;
            }
        };

        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        let model = StringList::new(&names);
        combo.set_model(Some(&model));

        let preferred = state_devices.storage.get_audio_device();
        set_preferred_device(&combo, &devices, preferred.as_ref());
    });

    let state_devices = Arc::clone(state);
    device_combo.connect_selected_notify(move |combo| {
        let idx = combo.selected();
        let state = Arc::clone(&state_devices);
        spawn_future_local(async move {
            set_audio_device(&state, idx).await;
        });
    });

    output_group.add(&device_combo);

    let mode_model = StringList::new(&["Resampled", "BitPerfect"]);
    let mode_combo = ComboRow::builder()
        .title("Output Mode")
        .subtitle(
            "Resampled: software volume, sample rate conversion; BitPerfect: no resampling, \
             hardware volume via ALSA mixer",
        )
        .model(&mode_model)
        .build();
    mode_combo.set_selected(match state.storage.get_output_mode() {
        Resampled => 0,
        BitPerfect => 1,
    });

    let state_mode = Arc::clone(state);
    mode_combo.connect_selected_notify(move |combo| {
        let mode = if combo.selected() == 0 {
            Resampled
        } else {
            BitPerfect
        };
        info!(
            output_mode = ?mode,
            "Output mode changed",
        );
        if let Err(e) = state_mode.playback.set_output_mode(mode) {
            warn!(error = %e, "Failed to set output mode");
        }
        spawn_future_local(persist_output_mode(Arc::clone(&state_mode.storage), mode));
    });

    output_group.add(&mode_combo);
    page.add(&output_group);

    build_playback_group(&page, state);

    dialog.add(&page);
}

/// Build the Playback preferences group.
fn build_playback_group(page: &PreferencesPage, state: &Arc<AppState>) {
    let playback_group = PreferencesGroup::new();
    playback_group.set_title("Playback");
    playback_group.set_description(Some("Playback behavior"));

    let initial_vol = (state.storage.get_settings_volume() * 100.0).round();
    let adjustment = Adjustment::new(initial_vol, 0.0, 100.0, 1.0, 10.0, 0.0);
    let volume_row = SpinRow::builder()
        .title("Volume")
        .subtitle("Playback volume level (0\u{2013}100)")
        .adjustment(&adjustment)
        .digits(0)
        .build();

    let state_vol = Arc::clone(state);
    volume_row.connect_notify_local(Some("value"), move |row, _| {
        let vol = row.value() / 100.0;
        if let Err(e) = state_vol.playback.set_volume(vol) {
            warn!(error = %e, "Failed to set volume from preferences");
        }
    });

    playback_group.add(&volume_row);

    let gapless_row = SwitchRow::new();
    gapless_row.set_title("Gapless Playback");
    gapless_row.set_subtitle("Seamless transitions between tracks");
    gapless_row.set_active(state.storage.get_gapless_enabled());

    let state_gapless = Arc::clone(state);
    gapless_row.connect_active_notify(move |row| {
        let enabled = row.is_active();
        if let Err(e) = state_gapless.playback.set_gapless_enabled(enabled) {
            warn!(error = %e, "Failed to toggle gapless playback");
        }
        spawn_future_local(save_gapless_setting(Arc::clone(&state_gapless), enabled));
    });

    playback_group.add(&gapless_row);
    page.add(&playback_group);
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ComboRow,
            gtk::{self, StringList, test},
            prelude::ComboRowExt,
        },
    };

    use crate::{
        playback::devices::DeviceInfo, ui::preferences::audio_setup::set_preferred_device,
    };

    fn mock_devices() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo {
                id: "pipewire".into(),
                name: "PipeWire".into(),
            },
            DeviceInfo {
                id: "hdmi".into(),
                name: "HDMI".into(),
            },
        ]
    }

    fn combo_with_devices() -> ComboRow {
        let combo = ComboRow::new();
        let model = StringList::new(&["PipeWire", "HDMI"]);
        combo.set_model(Some(&model));
        combo
    }

    #[test]
    fn set_preferred_device_selects_matching_index() -> Result<()> {
        let combo = combo_with_devices();
        let preferred = "HDMI".to_string();
        set_preferred_device(&combo, &mock_devices(), Some(&preferred));
        ensure!(combo.selected() == 1);
        Ok(())
    }

    #[test]
    fn set_preferred_device_leaves_default_on_no_match() -> Result<()> {
        let combo = combo_with_devices();
        let before = combo.selected();
        let preferred = "Missing".to_string();
        set_preferred_device(&combo, &mock_devices(), Some(&preferred));
        ensure!(combo.selected() == before);
        Ok(())
    }

    #[test]
    fn set_preferred_device_none_leaves_default() -> Result<()> {
        let combo = combo_with_devices();
        let before = combo.selected();
        set_preferred_device(&combo, &mock_devices(), None);
        ensure!(combo.selected() == before);
        Ok(())
    }
}

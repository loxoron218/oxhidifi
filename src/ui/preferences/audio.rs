//! Audio output and playback preferences pages.

use std::sync::Arc;

use {
    libadwaita::{
        ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage,
        gio::spawn_blocking,
        glib::spawn_future_local,
        gtk::{StringList, accessible::Property::Label},
        prelude::{
            AccessibleExtManual, ComboRowExt, PreferencesDialogExt, PreferencesGroupExt,
            PreferencesPageExt,
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
    ui::preferences::audio_playback::build_playback_group,
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
        "Audio device selection changed"
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

/// Persist output mode, logging on failure.
async fn persist_output_mode(storage: Arc<SqliteStorage>, mode: OutputMode) {
    if let Err(e) = storage.set_output_mode(mode).await {
        warn!(error = %e, "Failed to persist output mode");
    }
}

/// Build the Audio > Output and Audio > Playback group.
///
/// Combo rows keep the default `combo` class alongside custom ones: dropping
/// it leaves the selected value with a black background.
///
/// # Arguments
///
/// * `dialog` - Preferences dialog owning the new page.
/// * `state` - Application state for device and playback settings.
///
/// # Returns
///
/// * `PreferencesPage` - The audio page for visible-page selection.
pub fn build_audio_page(dialog: &PreferencesDialog, state: &Arc<AppState>) -> PreferencesPage {
    let page = PreferencesPage::builder()
        .title("Audio")
        .icon_name("audio-speakers-symbolic")
        .css_classes(["audio-page"])
        .tooltip_text("Audio output settings")
        .build();
    page.update_property(&[Label("Audio output settings")]);

    let output_group = PreferencesGroup::builder()
        .title("Output")
        .description("Audio output device")
        .css_classes(["output-group"])
        .tooltip_text("Audio output device settings")
        .build();

    let device_combo = ComboRow::builder()
        .title("Audio Device")
        .css_classes(["combo", "audio-device-row"])
        .tooltip_text("Select audio output device")
        .build();

    let state_devices = Arc::clone(state);
    let combo = device_combo.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let Ok(Ok(devices)) = spawn_blocking(list_output_devices).await else {
                warn!(
                    context = "audio-devices",
                    "Failed to enumerate audio devices"
                );
                return;
            };

            let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
            let model = StringList::new(&names);
            combo.set_model(Some(&model));

            let preferred = state_devices.storage.get_audio_device();
            set_preferred_device(&combo, &devices, preferred.as_ref());
        }));

    let state_selected = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(device_combo.connect_selected_notify(move |combo| {
            let idx = combo.selected();
            let state_cb = Arc::clone(&state_selected);
            state_selected
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    set_audio_device(&state_cb, idx).await;
                }));
        }));

    output_group.add(&device_combo);

    let mode_model = StringList::new(&["Resampled", "BitPerfect"]);
    let mode_selected = match state.storage.get_output_mode() {
        Resampled => 0,
        BitPerfect => 1,
    };
    let mode_combo = ComboRow::builder()
        .title("Output Mode")
        .subtitle(
            "Resampled: software volume, sample rate conversion; BitPerfect: no resampling, \
             hardware volume via ALSA mixer",
        )
        .model(&mode_model)
        .selected(mode_selected)
        .css_classes(["combo", "output-mode-row"])
        .tooltip_text("Select bit-perfect or resampled output")
        .build();

    let state_mode = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(mode_combo.connect_selected_notify(move |combo| {
            let mode = if combo.selected() == 0 {
                Resampled
            } else {
                BitPerfect
            };
            info!(output_mode = ?mode, "Output mode changed");
            if let Err(e) = state_mode.playback.set_output_mode(mode) {
                warn!(error = %e, "Failed to set output mode");
            }
            state_mode
                .handles
                .lock()
                .retain_task(spawn_future_local(persist_output_mode(
                    Arc::clone(&state_mode.storage),
                    mode,
                )));
        }));

    output_group.add(&mode_combo);
    page.add(&output_group);

    build_playback_group(&page, state);

    dialog.add(&page);
    page
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ComboRow, PreferencesDialog,
            glib::object::Cast,
            gtk::{self, StringList, Widget, accessible::Property::Label, test},
            prelude::{AccessibleExtManual, ComboRowExt, WidgetExt},
        },
    };

    use crate::{
        app::mocks::{find_descendants, isolated_app_state},
        playback::devices::DeviceInfo,
        storage::sort_rules::ArtistPlayOrder,
        ui::preferences::{
            audio::{build_audio_page, set_preferred_device},
            audio_playback::{artist_play_order_from_index, artist_play_order_to_index},
        },
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
        let model = StringList::new(&["PipeWire", "HDMI"]);
        let combo = ComboRow::builder()
            .model(&model)
            .css_classes(["combo", "test-device-row"])
            .tooltip_text("Test audio device selection")
            .build();
        combo.update_property(&[Label("Test audio device selection")]);
        combo
    }

    #[test]
    fn combo_row_keeps_combo_class() -> Result<()> {
        let combo = combo_with_devices();
        let classes = combo.css_classes();
        ensure!(
            classes.contains(&"combo".into()),
            "device combo must keep the default `combo` class"
        );
        ensure!(
            classes.contains(&"test-device-row".into()),
            "device combo must carry the `test-device-row` class"
        );
        Ok(())
    }

    #[test]
    fn audio_page_stays_in_focus_chain() -> Result<()> {
        let state = Arc::new(isolated_app_state()?);
        let dialog = PreferencesDialog::new();
        let page = build_audio_page(&dialog, &state);
        ensure!(
            page.can_focus(),
            "audio page must stay focusable so preferences rows keep keyboard focus for \
             Escape-to-close"
        );
        Ok(())
    }

    #[test]
    fn audio_page_combos_keep_combo_class() -> Result<()> {
        let state = Arc::new(isolated_app_state()?);
        let dialog = PreferencesDialog::new();
        let page = build_audio_page(&dialog, &state);
        let root: Widget = page.upcast();
        let combos = find_descendants::<ComboRow>(&root);
        ensure!(
            combos.len() == 3,
            "audio page must own the device, mode, and artist order combos"
        );
        for combo in &combos {
            ensure!(
                combo.css_classes().contains(&"combo".into()),
                "audio page combos must keep the default `combo` class"
            );
        }
        Ok(())
    }

    #[test]
    fn set_preferred_device_selects_matching_index() -> Result<()> {
        let combo = combo_with_devices();
        set_preferred_device(&combo, &mock_devices(), Some(&String::from("HDMI")));
        ensure!(combo.selected() == 1);
        Ok(())
    }

    #[test]
    fn set_preferred_device_leaves_default_on_no_match() -> Result<()> {
        let combo = combo_with_devices();
        let before = combo.selected();
        set_preferred_device(&combo, &mock_devices(), Some(&String::from("Missing")));
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

    #[test]
    fn artist_play_order_index_round_trips() -> Result<()> {
        for order in ArtistPlayOrder::options() {
            let idx = artist_play_order_to_index(order);
            ensure!(
                artist_play_order_from_index(idx) == Some(order),
                "idx {idx} must round-trip"
            );
        }
        ensure!(
            artist_play_order_from_index(99).is_none(),
            "unknown idx must be None"
        );
        Ok(())
    }
}

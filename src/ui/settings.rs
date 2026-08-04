//! `PreferencesDialog` for library directories, audio device selection,
//! view preferences, and gapless playback toggle per FR-033.

use std::{path::PathBuf, sync::Arc};

use {
    libadwaita::{
        ActionRow, ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage, SpinRow,
        SwitchRow,
        gio::{Cancellable, File, spawn_blocking},
        glib::{Error, spawn_future_local},
        gtk::{Adjustment, Button, FileDialog, StringList, Window, accessible::Property::Label},
        prelude::{
            AccessibleExtManual, ActionRowExt, AdwDialogExt, ButtonExt, ComboRowExt, FileExt,
            ObjectExt, PreferencesDialogExt, PreferencesGroupExt, PreferencesPageExt,
            PreferencesRowExt, WidgetExt,
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::AppState,
    playback::{
        control::PlaybackController,
        devices::{
            DeviceInfo,
            OutputMode::{self, BitPerfect, Resampled},
            list_output_devices,
        },
    },
    storage::{Storage, database::SqliteStorage, records::LibraryDirectory},
};

/// Remove a library directory by ID in a background task.
fn spawn_remove_directory(storage: &Arc<SqliteStorage>, dir_id: i64) {
    info!(dir_id, "Library directory removed",);
    let storage = Arc::clone(storage);
    spawn_future_local(async move {
        if let Err(e) = storage.remove_library_directory(dir_id).await {
            error!(error = %e, "Failed to remove library directory");
        }
    });
}

/// Add a library directory by path in a background task.
fn spawn_add_directory(state: &Arc<AppState>, path: PathBuf) {
    let state = Arc::clone(state);
    spawn_future_local(async move {
        if let Err(e) = state.storage.add_library_directory(&path).await {
            error!(error = %e, "Failed to add library directory");
        }
    });
}

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

/// Build a directory row with a remove button and add it to the group.
fn add_directory_row(
    group: &PreferencesGroup,
    storage: &Arc<SqliteStorage>,
    dir: &LibraryDirectory,
) {
    let row = ActionRow::builder()
        .title(&dir.path)
        .activatable_widget(group)
        .build();
    let remove_btn = Button::builder()
        .label("Remove")
        .use_underline(true)
        .css_classes(["destructive-action", "flat"])
        .tooltip_text("Remove this directory from the library")
        .can_focus(true)
        .build();
    remove_btn.update_property(&[Label("Remove directory")]);
    row.add_suffix(&remove_btn);
    row.set_activatable_widget(Some(&remove_btn));

    let storage = Arc::clone(storage);
    let dir_id = dir.id;
    let row_clone = row.clone();
    remove_btn.connect_clicked(move |_| {
        spawn_remove_directory(&storage, dir_id);
        row_clone.set_visible(false);
    });

    group.add(&row);
}

/// Handle folder selection result from the file dialog.
fn on_folder_selected(state: &Arc<AppState>, result: Result<File, Error>) {
    if let Ok(file) = result
        && let Some(path) = file.path()
    {
        spawn_add_directory(state, path.clone());
        info!(path = %path.display(), "Library directory added");
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

/// Build and present the preferences dialog.
pub fn show_preferences_dialog(state: &Arc<AppState>, parent: &Window) {
    let dialog = PreferencesDialog::new();
    dialog.set_search_enabled(false);

    build_library_page(&dialog, state, parent);
    build_audio_page(&dialog, state);
    build_view_page(&dialog, state);

    dialog.present(Some(parent));
}

/// Build the Library > Directories page.
/// Persist gapless playback setting, logging on failure.
async fn save_gapless_setting(state: Arc<AppState>, enabled: bool) {
    if let Err(e) = state.storage.set_gapless_enabled(enabled).await {
        error!(error = %e, "Failed to save gapless setting");
    }
}

/// Persist album labels visibility setting and trigger a grid rebuild.
async fn save_album_labels_setting(state: Arc<AppState>, enabled: bool) {
    if let Err(e) = state.storage.set_show_album_labels(enabled).await {
        warn!(error = %e, "Failed to save album labels setting");
    }
    if let Err(e) = state.refresh_tx.send(()) {
        warn!(error = %e, "Failed to send refresh signal");
    }
}

/// Persist output mode, logging on failure.
async fn persist_output_mode(storage: Arc<SqliteStorage>, mode: OutputMode) {
    if let Err(e) = storage.set_output_mode(mode).await {
        warn!(error = %e, "Failed to persist output mode");
    }
}

/// Build the Library > Directories page.
fn build_library_page(dialog: &PreferencesDialog, state: &Arc<AppState>, parent: &Window) {
    let page = PreferencesPage::new();
    page.set_title("Library");
    page.set_icon_name(Some("folder-music-symbolic"));

    let group = PreferencesGroup::new();
    group.set_title("Directories");
    group.set_description(Some("Music directories to scan for audio files"));

    let state_clone = Arc::clone(state);
    let group_clone = group.clone();
    spawn_future_local(async move {
        let dirs = match state_clone.storage.list_library_directories().await {
            Ok(d) => d,
            Err(e) => {
                error!(error = %e, "Failed to list library directories");
                return;
            }
        };

        for dir in &dirs {
            add_directory_row(&group_clone, &state_clone.storage, dir);
        }
    });

    let add_btn = Button::builder()
        .label("Add Directory")
        .use_underline(true)
        .css_classes(["suggested-action"])
        .tooltip_text("Add a new music directory to scan")
        .can_focus(true)
        .build();
    add_btn.update_property(&[Label("Add Directory")]);
    group.add(&add_btn);

    let state_clone = Arc::clone(state);
    let parent_clone = parent.clone();
    add_btn.connect_clicked(move |_| {
        let dialog = FileDialog::builder()
            .title("Select Music Directory")
            .accept_label("Select")
            .build();
        let state = Arc::clone(&state_clone);
        dialog.select_folder(Some(&parent_clone), None::<&Cancellable>, move |result| {
            on_folder_selected(&state, result);
        });
    });

    page.add(&group);
    dialog.add(&page);
}

/// Build the Audio > Output and Audio > Playback group.
fn build_audio_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
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

/// Build the View > Display page.
fn build_view_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
    let page = PreferencesPage::new();
    page.set_title("View");
    page.set_icon_name(Some("preferences-desktop-display-symbolic"));

    let display_group = PreferencesGroup::new();
    display_group.set_title("Display");
    display_group.set_description(Some("Grid card appearance"));

    let labels_row = SwitchRow::new();
    labels_row.set_title("Album Labels");
    labels_row.set_subtitle("Show album title, artist, and format under cover art");
    labels_row.set_active(state.storage.get_show_album_labels());

    let state_labels = Arc::clone(state);
    labels_row.connect_active_notify(move |row| {
        let enabled = row.is_active();
        spawn_future_local(save_album_labels_setting(
            Arc::clone(&state_labels),
            enabled,
        ));
    });

    display_group.add(&labels_row);
    page.add(&display_group);

    dialog.add(&page);
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

    use crate::{playback::devices::DeviceInfo, ui::settings::set_preferred_device};

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

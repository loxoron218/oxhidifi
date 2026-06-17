//! `PreferencesDialog` for library directories, audio device selection,
//! view preferences, and gapless playback toggle per FR-033.

use std::{path::PathBuf, sync::Arc};

use {
    libadwaita::{
        ActionRow, ComboRow, PreferencesDialog, PreferencesGroup, PreferencesPage, SpinRow,
        SwitchRow,
        gio::{Cancellable, File, spawn_blocking},
        glib::{Error, spawn_future_local},
        gtk::{Adjustment, Button, FileDialog, StringList, Window},
        prelude::{
            ActionRowExt, AdwDialogExt, ButtonExt, ComboRowExt, FileExt, ObjectExt,
            PreferencesDialogExt, PreferencesGroupExt, PreferencesPageExt, PreferencesRowExt,
            WidgetExt,
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::AppState,
    playback::{
        engine::PlaybackController,
        output::{DeviceInfo, list_output_devices},
    },
    storage::{
        LibraryDirectory, Storage,
        database::SqliteStorage,
        settings::{
            ActiveTab::{Albums, Artists},
            ViewMode::{Column, Grid},
        },
    },
};

/// Remove a library directory by ID in a background task.
fn spawn_remove_directory(storage: &Arc<SqliteStorage>, dir_id: i64) {
    info!(
        target: "ui::settings",
        dir_id,
        "Library directory removed",
    );
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
        target: "ui::settings",
        audio_device = name.as_deref().unwrap_or("default"),
        "Audio device selection changed",
    );
    if let Err(e) = state.storage.set_audio_device(name) {
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
        .css_classes(["destructive-action", "flat"])
        .build();
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
pub fn show_preferences_dialog(state: &Arc<AppState>) {
    let dialog = PreferencesDialog::new();
    dialog.set_search_enabled(false);

    build_library_page(&dialog, state);
    build_audio_page(&dialog, state);
    build_view_page(&dialog, state);

    let parent: Option<&Window> = None;
    dialog.present(parent);
}

/// Build the Library > Directories page.
fn build_library_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
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
        .css_classes(["suggested-action"])
        .build();
    group.add(&add_btn);

    let state_clone = Arc::clone(state);
    add_btn.connect_clicked(move |_| {
        let dialog = FileDialog::builder()
            .title("Select Music Directory")
            .accept_label("Select")
            .build();
        let state = Arc::clone(&state_clone);
        dialog.select_folder(None::<&Window>, None::<&Cancellable>, move |result| {
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
        let Ok(Ok(devices)) = spawn_blocking(list_output_devices).await else {
            warn!("Failed to enumerate audio devices");
            return;
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
    page.add(&output_group);

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
        if let Err(e) = state_gapless.storage.set_gapless_enabled(enabled) {
            error!(error = %e, "Failed to save gapless setting");
        }
    });

    playback_group.add(&gapless_row);
    page.add(&playback_group);

    dialog.add(&page);
}

/// Build the View > Display page.
fn build_view_page(dialog: &PreferencesDialog, state: &Arc<AppState>) {
    let page = PreferencesPage::new();
    page.set_title("View");
    page.set_icon_name(Some("preferences-desktop-display-symbolic"));

    let display_group = PreferencesGroup::new();
    display_group.set_title("Display");
    display_group.set_description(Some("Default view preferences"));

    let view_model = StringList::new(&["Grid", "Column"]);
    let view_combo = ComboRow::builder()
        .title("Default View")
        .model(&view_model)
        .build();
    view_combo.set_selected(match state.storage.get_view_mode() {
        Grid => 0,
        Column => 1,
    });

    let state_view = Arc::clone(state);
    view_combo.connect_selected_notify(move |combo| {
        let mode = if combo.selected() == 0 { Grid } else { Column };
        info!(
            target: "ui::settings",
            view_mode = if matches!(mode, Grid) { "grid" } else { "column" },
            "Default view mode changed",
        );
        if let Err(e) = state_view.storage.set_view_mode(mode) {
            error!(error = %e, "Failed to save view mode");
        }
    });

    display_group.add(&view_combo);

    let tab_model = StringList::new(&["Albums", "Artists"]);
    let tab_combo = ComboRow::builder()
        .title("Default Tab")
        .model(&tab_model)
        .build();
    tab_combo.set_selected(match state.storage.get_active_tab() {
        Albums => 0,
        Artists => 1,
    });

    let state_tab = Arc::clone(state);
    tab_combo.connect_selected_notify(move |combo| {
        let tab = if combo.selected() == 0 {
            Albums
        } else {
            Artists
        };
        info!(
            target: "ui::settings",
            active_tab = if matches!(tab, Albums) { "albums" } else { "artists" },
            "Default tab changed",
        );
        if let Err(e) = state_tab.storage.set_active_tab(tab) {
            error!(error = %e, "Failed to save active tab");
        }
        state_tab.active_tab_tx.send_if_modified(|current| {
            let changed = *current != tab;
            *current = tab;
            changed
        });
    });

    display_group.add(&tab_combo);
    page.add(&display_group);
    dialog.add(&page);
}

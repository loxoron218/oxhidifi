//! Library directories preferences page.

use std::{path::PathBuf, sync::Arc};

use {
    libadwaita::{
        ActionRow, PreferencesDialog, PreferencesGroup, PreferencesPage,
        gio::{Cancellable, File},
        glib::{Error, spawn_future_local},
        gtk::{Button, FileDialog, Window, accessible::Property::Label},
        prelude::{
            AccessibleExtManual, ActionRowExt, ButtonExt, FileExt, PreferencesDialogExt,
            PreferencesGroupExt, PreferencesPageExt, WidgetExt,
        },
    },
    tracing::{error, info},
};

use crate::{
    app::AppState,
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

/// Build the Library > Directories page.
pub fn build_library_page(dialog: &PreferencesDialog, state: &Arc<AppState>, parent: &Window) {
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

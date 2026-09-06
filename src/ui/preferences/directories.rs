//! Library directories preferences page.

use std::{
    path::{Path, PathBuf},
    slice::from_ref,
    sync::Arc,
};

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
    tokio::spawn,
    tracing::{error, info, warn},
};

use crate::{
    app::runtime::AppState,
    library::{scanner::LibraryScanner, watcher::LibraryWatcher},
    storage::{Storage, catalog::LibraryDirectory, database::SqliteStorage},
};

/// Remove a library directory by ID, hard-delete tracks/albums/artists, unwatch, and refresh UI.
fn spawn_remove_directory(state: &Arc<AppState>, dir_id: i64, dir_path: String) {
    info!(dir_id, path = %dir_path, "Library directory removed");
    let state_clone = Arc::clone(state);
    spawn_future_local(async move {
        if let Err(e) = state_clone.storage.remove_library_directory(dir_id).await {
            error!(error = %e, "Failed to remove library directory");
            return;
        }
        let watcher_opt = state_clone.watcher.lock().as_ref().cloned();
        if let Some(watcher_arc) = watcher_opt {
            let path = Path::new(&dir_path).to_path_buf();
            try_unwatch_directory(&watcher_arc, &path, &dir_path);
        }
        state_clone.refresh.publish();
        info!(path = %dir_path, "Refresh published after directory removal");
    });
}

/// Attempt to unwatch a directory, logging on failure.
fn try_unwatch_directory(watcher: &LibraryWatcher<SqliteStorage>, path: &Path, dir_path: &str) {
    if let Err(error) = watcher.unwatch_directory(path) {
        warn!(
            error = %error,
            path = %dir_path,
            "Failed to unwatch removed directory"
        );
    }
}

/// Add a library directory by path in a background task.
fn spawn_add_directory(state: &Arc<AppState>, path: PathBuf) {
    let state = Arc::clone(state);
    spawn_future_local(handle_add_directory(state, path));
}

/// Handle adding a directory, then scanning and refreshing the UI.
async fn handle_add_directory(state: Arc<AppState>, path: PathBuf) {
    if let Err(e) = state.storage.add_library_directory(&path).await {
        error!(error = %e, "Failed to add library directory");
        return;
    }
    if let Some(watcher_arc) = state.watcher.lock().as_ref().cloned()
        && let Err(e) = watcher_arc.watch_directories(from_ref(&path))
    {
        warn!(error = %e, path = %path.display(), "Failed to watch new directory");
    }
    info!(
        path = %path.display(),
        "Added library directory, spawning background scan"
    );
    scan_directory_and_publish(state, path).await;
}

/// Scan a directory and publish a library refresh.
async fn scan_directory_and_publish(state: Arc<AppState>, path: PathBuf) {
    let scanner = Arc::clone(&state.scanner);
    let refresh = state.refresh.clone();
    let scan_path = path.clone();
    spawn(async move {
        if let Err(e) = scanner.scan_directory(&scan_path).await {
            warn!(error = %e, path = %scan_path.display(), "Failed to scan directory");
            return;
        }
        info!(path = %scan_path.display(), "Scan completed after adding directory");
        refresh.publish();
    })
    .await
    .unwrap_or_else(|e| {
        warn!(error = %e, "Scan task panicked");
    });
}

/// Build a directory row with a remove button and add it to the group.
fn add_directory_row(group: &PreferencesGroup, state: &Arc<AppState>, dir: &LibraryDirectory) {
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

    let state_clone = Arc::clone(state);
    let dir_id = dir.id;
    let dir_path = dir.path.clone();
    let row_clone = row.clone();
    remove_btn.connect_clicked(move |_| {
        spawn_remove_directory(&state_clone, dir_id, dir_path.clone());
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
            add_directory_row(&group_clone, &state_clone, dir);
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

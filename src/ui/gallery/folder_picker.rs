//! Folder import flow for empty library states.
//!
//! Provides the "Add Music Folder" button, file chooser dialog,
//! and background scan trigger used by empty states.

use std::{path::PathBuf, sync::Arc};

use {
    libadwaita::{
        glib::{object::Cast, spawn_future_local},
        gtk::{Button, FileDialog, Window, accessible::Property::Label},
        prelude::{AccessibleExtManual, ButtonExt, FileExt, WidgetExt},
    },
    tokio::spawn,
    tracing::{info, warn},
};

use crate::{
    app::runtime::AppState,
    library::scanner::{FsScanner, LibraryScanner},
    storage::{Storage, database::SqliteStorage},
    ui::signal_handlers::ValueSignal,
};

/// Build the "Add Music Folder" button with click handler.
///
/// Creates a styled button that opens a file chooser when clicked.
///
/// # Arguments
///
/// * `state` - Application state for the click handler
pub fn build_add_folder_button(state: &Arc<AppState>) -> Button {
    let add_folder_button = Button::builder()
        .label("Add Music Folder")
        .use_underline(true)
        .css_classes(["suggested-action"])
        .can_focus(true)
        .tooltip_text("Open a file chooser to select your music folder")
        .build();
    add_folder_button.update_property(&[Label("Add Music Folder")]);

    let state_clone = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(add_folder_button.connect_clicked(move |btn| {
            let state = Arc::clone(&state_clone);
            let parent = parent_window(btn);
            add_music_folder(&state, parent);
        }));

    add_folder_button
}

/// Get the parent window from a button's root widget.
fn parent_window(btn: &Button) -> Option<Window> {
    let r = btn.root()?;
    r.downcast::<Window>().map_or_else(
        |_| {
            warn!("Button has no parent window");
            None
        },
        Some,
    )
}

/// Open a file chooser dialog to add a music folder.
///
/// Adds the directory to storage and spawns a background scan.  Runs on the
/// `GLib` main context via a local future so the dialog and widgets are only
/// touched on the main thread.
fn add_music_folder(state: &Arc<AppState>, parent: Option<Window>) {
    let state_cb = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let dialog = FileDialog::builder()
                .title("Select Music Folder")
                .accept_label("Add Folder")
                .build();

            let folder = match dialog.select_folder_future(parent.as_ref()).await {
                Ok(folder) => folder,
                Err(e) => {
                    warn!(error = %e, "File chooser cancelled or failed");
                    return;
                }
            };

            let Some(path) = folder.path() else {
                info!("No folder path selected");
                return;
            };

            if let Err(e) = state_cb.storage.add_library_directory(&path).await {
                warn!(error = %e, path = %path.display(), "Failed to add library directory");
                return;
            }

            info!(path = %path.display(), "Added library directory, spawning background scan");

            let scanner = Arc::clone(&state_cb.scanner);
            let scan_path = path.clone();
            let refresh = state_cb.refresh.clone();
            drop(spawn(scan_directory_and_refresh(
                scanner, scan_path, refresh,
            )));
        }));
}

/// Scan a library directory in the background and publish a library refresh.
async fn scan_directory_and_refresh(
    scanner: Arc<FsScanner<SqliteStorage>>,
    path: PathBuf,
    refresh: ValueSignal<()>,
) {
    if let Err(e) = scanner.scan_directory(&path).await {
        warn!(error = %e, path = %path.display(), "Failed to scan directory");
        return;
    }
    info!(path = %path.display(), "Scan completed");
    refresh.publish();
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, Button, test},
    };

    use crate::ui::gallery::folder_picker::parent_window;

    #[test]
    fn parent_window_none_when_unattached() -> Result<()> {
        let button = Button::new();
        ensure!(parent_window(&button).is_none());
        Ok(())
    }
}

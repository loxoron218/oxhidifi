//! Shared utilities for library views.
//!
//! Provides empty state components and the generic grid builder
//! used by the album and artist grid views.

use std::sync::Arc;

use {
    libadwaita::{
        ApplicationWindow,
        glib::spawn_future_local,
        gtk::{
            Align::Center, Box, Button, FileDialog, Image, Label, Orientation::Vertical,
            ScrolledWindow, Stack, Widget, accessible::Property::Label as PropertyLabel,
            prelude::WidgetExt,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, FileExt, IsA},
    },
    parking_lot::Mutex,
    tokio::spawn,
    tracing::info,
};

use crate::{
    app::AppState,
    library::scanner::LibraryScanner,
    storage::{Storage, settings::ViewMode},
    ui::library::column_view::NarrowState,
};

/// Parameters for building an empty state view.
pub struct EmptyStateParams {
    /// Icon name for the empty state.
    pub icon_name: &'static str,
    /// Accessible label for the icon.
    pub icon_label: &'static str,
    /// Heading text.
    pub heading: &'static str,
    /// Accessible label for the heading.
    pub heading_label: &'static str,
    /// Description text.
    pub description: &'static str,
    /// Accessible label for the description.
    pub description_label: &'static str,
}

/// A built library grid view with a `Stack` holding both grid and column
/// children, so view-mode switching only toggles visibility — no rebuild.
///
/// Each mode child (`"grid"` and `"column"`) is itself a `ScrolledWindow`
/// so each mode retains its own scroll position independently.
pub struct LibraryGrid {
    /// `Stack` containing `"grid"` and `"column"` children.
    /// Toggling the visible child switches modes instantly.
    pub mode_stack: Stack,
    /// Tracks which [`ViewMode`] this view was last built with.
    pub current_mode: Arc<Mutex<ViewMode>>,
}

/// Build an empty state with icon, heading, description, and add-folder button.
///
/// # Arguments
///
/// * `state` - Application state
/// * `params` - Configuration for the empty state content
#[must_use]
pub fn build_empty_state(state: &Arc<AppState>, params: &EmptyStateParams) -> Box {
    let container = build_empty_container(state);

    let icon = Image::builder()
        .icon_name(params.icon_name)
        .pixel_size(96)
        .css_classes(["icon-drop-shadow", "dim-label"])
        .build();
    icon.update_property(&[PropertyLabel(params.icon_label)]);

    let heading = Label::builder()
        .label(params.heading)
        .css_classes(["title-1", "accent"])
        .build();
    heading.update_property(&[PropertyLabel(params.heading_label)]);

    let description = Label::builder()
        .label(params.description)
        .wrap(true)
        .max_width_chars(40)
        .css_classes(["dim-label", "body"])
        .build();
    description.update_property(&[PropertyLabel(params.description_label)]);

    let add_folder_button = build_add_folder_button(state);

    container.append(&icon);
    container.append(&heading);
    container.append(&description);
    container.append(&add_folder_button);

    container
}

/// Build a library grid view that pre-builds both grid (`FlowBox`) and column
/// (`ColumnView`) layouts inside a `Stack`.  The parent orchestrator toggles
/// the stack's visible child on view‑mode change — no data re‑fetch or widget
/// reconstruction.
///
/// Calls `setup_fn` asynchronously to populate the stack.
/// The `setup_fn` is responsible for:
///
/// 1. Populating the given `Stack` with named children `"grid"` and `"column"`
/// 2. Calling `set_visible_child_name` for `initial_mode`
///
/// # Arguments
///
/// * `state` - Application state
/// * `tooltip` - Tooltip text for the grid
/// * `narrow_mode` - Narrow‑width tracker for adaptive column hiding
/// * `setup_fn` - Closure that populates a `Stack` with both views; receives `(&Stack, state,
///   narrow_state, initial_mode)`.  Called once at startup and again on library refresh to
///   re-populate in‑place.
#[must_use]
pub fn build_library_grid(
    state: &Arc<AppState>,
    _tooltip: &str,
    narrow_state: &Arc<NarrowState>,
    setup_fn: impl Fn(&Stack, Arc<AppState>, Arc<NarrowState>, ViewMode) + Clone + 'static,
) -> LibraryGrid {
    let initial_mode = *state.view_mode_tx.borrow();
    let current_mode = Arc::new(Mutex::new(initial_mode));
    let nm = Arc::clone(narrow_state);
    let mode_stack = Stack::new();
    setup_fn(&mode_stack, Arc::clone(state), nm, initial_mode);

    let mut refresh_rx = state.refresh_tx.subscribe();
    let refresh_state = Arc::clone(state);
    let refresh_mode_stack = mode_stack.clone();
    let refresh_setup = setup_fn;
    let refresh_nm = Arc::clone(narrow_state);
    let refresh_mode = Arc::clone(&current_mode);
    spawn_future_local(async move {
        while refresh_rx.changed().await.is_ok() {
            let mode = *refresh_state.view_mode_tx.borrow();
            clear_stack(&refresh_mode_stack);
            refresh_setup(
                &refresh_mode_stack,
                Arc::clone(&refresh_state),
                Arc::clone(&refresh_nm),
                mode,
            );
            update_mode(&refresh_mode, mode);
        }
    });

    LibraryGrid {
        mode_stack,
        current_mode,
    }
}

/// Remove all children from a `Stack`.
fn clear_stack(stack: &Stack) {
    while let Some(child) = stack.first_child() {
        stack.remove(&child);
    }
}

/// Wrap `child` in a `ScrolledWindow` and add it to `stack` as a named page.
pub fn add_scrolled(stack: &Stack, child: &impl IsA<Widget>, name: &str) {
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();
    scrolled.set_child(Some(child));
    stack.add_named(&scrolled, Some(name));
}

/// Update the tracked view mode, ignoring a poisoned mutex.
fn update_mode(mode_arc: &Arc<Mutex<ViewMode>>, mode: ViewMode) {
    *mode_arc.lock() = mode;
}

/// Remove all children from a `Box`.
pub fn clear_container(container: &Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

/// Build an empty state container with consistent styling.
///
/// Creates a vertically centered box that can be populated with
/// empty state content (icon, text, buttons).
///
/// # Arguments
///
/// * `_state` - Application state (reserved for future use)
#[must_use]
pub fn build_empty_container(_state: &Arc<AppState>) -> Box {
    Box::builder()
        .orientation(Vertical)
        .spacing(18)
        .valign(Center)
        .halign(Center)
        .vexpand(true)
        .hexpand(true)
        .build()
}

/// Build the "Add Music Folder" button with click handler.
///
/// Creates a styled button that opens a file chooser when clicked.
///
/// # Arguments
///
/// * `state` - Application state for the click handler
#[must_use]
pub fn build_add_folder_button(state: &Arc<AppState>) -> Button {
    let add_folder_button = Button::builder()
        .label("Add Music Folder")
        .css_classes(["suggested-action"])
        .can_focus(true)
        .tooltip_text("Open a file chooser to select your music folder")
        .build();

    let state_clone = Arc::clone(state);
    add_folder_button.connect_clicked(move |_| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            add_music_folder(&state).await;
        });
    });

    add_folder_button
}

/// Open a file chooser dialog to add a music folder.
///
/// Adds the directory to storage and spawns a background scan.
async fn add_music_folder(state: &AppState) {
    let dialog = FileDialog::builder()
        .title("Select Music Folder")
        .accept_label("Add Folder")
        .build();

    let folder = match dialog
        .select_folder_future(Option::<&ApplicationWindow>::None)
        .await
    {
        Ok(folder) => folder,
        Err(e) => {
            info!(error = %e, "File chooser cancelled or failed");
            return;
        }
    };

    let Some(path) = folder.path() else {
        info!("No folder path selected");
        return;
    };

    if let Err(e) = state.storage.add_library_directory(&path).await {
        info!(error = %e, path = %path.display(), "Failed to add library directory");
        return;
    }

    info!(path = %path.display(), "Added library directory, spawning background scan");

    let scanner = Arc::clone(&state.scanner);
    let scan_path = path.clone();
    let refresh_tx = state.refresh_tx.clone();
    spawn(async move {
        if let Err(e) = scanner.scan_directory(&scan_path).await {
            info!(error = %e, path = %scan_path.display(), "Failed to scan directory");
            return;
        }
        info!(path = %scan_path.display(), "Scan completed");
        if let Err(e) = refresh_tx.send(()) {
            info!(error = %e, "Failed to send refresh signal");
        }
    });
}

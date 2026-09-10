//! Shared utilities for library views.
//!
//! Provides empty state components and the generic grid builder
//! used by the album and artist grid views.

use std::sync::{Arc, atomic::Ordering::Relaxed};

use {
    async_channel::Receiver,
    libadwaita::{
        glib::spawn_future_local,
        gtk::{
            Align::Center, Box, Image, Label, Orientation::Vertical, ScrolledWindow, Stack, Widget,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, IsA, WidgetExt},
    },
    parking_lot::Mutex,
};

use crate::{
    app::runtime::AppState,
    storage::{Storage, view_mode::ViewMode},
    ui::gallery::{folder_picker::build_add_folder_button, narrow_flag::NarrowState},
};

/// Parameters for building an empty state view.
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug)]
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

/// Build a library grid view that pre-builds both the grid (`FlowBox`) and
/// column (`ColumnView`) layouts inside a `Stack`.
///
/// The parent orchestrator toggles the stack's visible child on view‑mode
/// change — no data re‑fetch or widget reconstruction.
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
/// * `narrow_mode` - Narrow‑width tracker for adaptive column hiding
/// * `setup_fn` - Closure that populates a `Stack` with both views; receives `(&Stack, state,
///   narrow_state, initial_mode)`.  Called once at startup and again on library refresh to
///   re-populate in‑place.
pub fn build_library_grid(
    state: &Arc<AppState>,
    narrow_state: &Arc<NarrowState>,
    setup_fn: impl Fn(&Stack, Arc<AppState>, Arc<NarrowState>, ViewMode) + Clone + 'static,
) -> LibraryGrid {
    let initial_mode = state.view_mode.borrow();
    let current_mode = Arc::new(Mutex::new(initial_mode));
    let nm = Arc::clone(narrow_state);
    let mode_stack = Stack::new();
    setup_fn(&mode_stack, Arc::clone(state), nm, initial_mode);

    let refresh_rx = state.refresh.subscribe();
    let refresh_state = Arc::clone(state);
    let refresh_mode_stack = mode_stack.clone();
    let refresh_setup = setup_fn;
    let refresh_nm = Arc::clone(narrow_state);
    let refresh_mode = Arc::clone(&current_mode);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            while refresh_rx.recv().await.is_ok() {
                drain_receiver(&refresh_rx);
                let mode = refresh_state.view_mode.borrow();
                *refresh_state.album_grid.cache.lock() = None;
                *refresh_state.artist_grid.cache.lock() = None;
                _ = refresh_state.album_grid.generation.fetch_add(1, Relaxed);
                _ = refresh_state.artist_grid.generation.fetch_add(1, Relaxed);
                clear_stack(&refresh_mode_stack);
                refresh_setup(
                    &refresh_mode_stack,
                    Arc::clone(&refresh_state),
                    Arc::clone(&refresh_nm),
                    mode,
                );
                update_mode(&refresh_mode, mode);
            }
        }));

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

/// Drain any pending values from a receiver without blocking.
fn drain_receiver<T>(receiver: &Receiver<T>) {
    while receiver.try_recv().is_ok() {}
}

/// Wrap `child` in a `ScrolledWindow` and add it to `stack` as a named page.
pub fn add_scrolled(stack: &Stack, child: &impl IsA<Widget>, name: &str) {
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();
    scrolled.set_child(Some(child));
    drop(stack.add_named(&scrolled, Some(name)));
}

/// Show the library empty state for a grid view.
///
/// Shared helper for album/artist grids to avoid duplicating the
/// `list_library_directories` fetch and empty-state widget construction.
/// Checks for an existing `"grid"` child first (race-guard), then
/// asynchronously fetches library directories and displays the appropriate
/// empty state (configured vs. empty library).
pub fn show_library_empty(
    state: &Arc<AppState>,
    stack: &Stack,
    icon_name: &'static str,
    icon_label: &'static str,
) {
    if stack.child_by_name("grid").is_some() {
        stack.set_visible_child_name("grid");
        return;
    }
    let state_clone = Arc::clone(state);
    let stack_clone = stack.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            let dirs = state_clone
                .storage
                .list_library_directories()
                .await
                .unwrap_or_default();
            let params = if dirs.is_empty() {
                EmptyStateParams {
                    icon_name,
                    icon_label,
                    heading: "No Music Library Configured",
                    heading_label: "No music library configured",
                    description: "Add a music folder via Preferences > Library to get started.",
                    description_label: "Add a music folder via Preferences to get started.",
                }
            } else {
                EmptyStateParams {
                    icon_name,
                    icon_label,
                    heading: "No Music Found",
                    heading_label: "No music found",
                    description: "No music files found in your library directories. Add more \
                                  files or check your folders.",
                    description_label: "No music files found",
                }
            };
            let empty_widget = build_empty_state(&state_clone, &params);
            if stack_clone.child_by_name("grid").is_none() {
                drop(stack_clone.add_named(&empty_widget, Some("grid")));
            }
            stack_clone.set_visible_child_name("grid");
        }));
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
/// * `state` - Application state (reserved for future use)
#[must_use]
pub fn build_empty_container(_: &Arc<AppState>) -> Box {
    Box::builder()
        .orientation(Vertical)
        .spacing(18)
        .valign(Center)
        .halign(Center)
        .vexpand(true)
        .hexpand(true)
        .build()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, Box, Label, Orientation::Vertical, test},
            prelude::{BoxExt, WidgetExt},
        },
        parking_lot::Mutex,
    };

    use crate::{
        storage::view_mode::ViewMode::{Column, Grid},
        ui::gallery::empty::{clear_container, update_mode},
    };

    #[test]
    fn update_mode_sets_mutex_value() -> Result<()> {
        let mode = Arc::new(Mutex::new(Grid));
        update_mode(&mode, Column);
        ensure!(*mode.lock() == Column);
        Ok(())
    }

    #[test]
    fn clear_container_removes_children() -> Result<()> {
        let container = Box::new(Vertical, 0);
        container.append(&Label::new(Some("child")));
        ensure!(
            container.first_child().is_some(),
            "container must hold a child before clearing"
        );
        clear_container(&container);
        ensure!(
            container.first_child().is_none(),
            "container must be empty after clearing"
        );
        Ok(())
    }
}

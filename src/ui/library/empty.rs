//! Shared utilities for library views.
//!
//! Provides empty state components and the generic grid builder
//! used by the album and artist grid views.

use std::sync::Arc;

use {
    libadwaita::{
        ApplicationWindow,
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::{Center, Start},
            Box, Button, FileDialog, FlowBox, Image, Label, ListBox, ListBoxRow,
            Orientation::Vertical,
            ScrolledWindow,
            SelectionMode::None,
            Widget,
            accessible::Property::Label as PropertyLabel,
            prelude::WidgetExt,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, FileExt},
    },
    tokio::spawn,
    tracing::info,
};

use crate::{
    app::AppState,
    library::scanner::LibraryScanner,
    storage::{
        Storage,
        settings::ViewMode::{self, Column, Grid},
    },
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

/// Build a library grid view with a `FlowBox` or `ListBox` inside a `ScrolledWindow`.
///
/// Spawns the given loader function asynchronously to populate the grid.
/// Watches for library refresh signals and view mode changes, and re-renders
/// automatically. Switches between `FlowBox` (grid) and `ListBox` (column)
/// based on the current view mode.
///
/// View mode changes only rearrange existing card widgets — they do NOT
/// re-query the database or re-create widgets, eliminating UI freezes.
///
/// # Arguments
///
/// * `state` - Application state
/// * `tooltip` - Tooltip text for the grid
/// * `load_fn` - Closure that populates the container asynchronously; receives `(state,
///   container_box, scrolled, view_mode)`. The closure must clear and rebuild `container_box`
///   children on each call.
#[must_use]
pub fn build_library_grid(
    state: &Arc<AppState>,
    _tooltip: &str,
    load_fn: impl Fn(Arc<AppState>, Box, ScrolledWindow, ViewMode) + Clone + 'static,
) -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let container = Box::builder().orientation(Vertical).build();

    let initial_mode = *state.view_mode_tx.borrow();
    load_fn(
        Arc::clone(state),
        container.clone(),
        scrolled.clone(),
        initial_mode,
    );

    scrolled.set_child(Some(&container));

    let mut view_rx = state.view_mode_tx.subscribe();
    let view_container = container.clone();
    spawn_future_local(async move {
        while view_rx.changed().await.is_ok() {
            let mode = *view_rx.borrow();
            switch_layout(&view_container, mode);
        }
    });

    let mut refresh_rx = state.refresh_tx.subscribe();
    let refresh_state = Arc::clone(state);
    let refresh_container = container;
    let refresh_scrolled = scrolled.clone();
    let refresh_load = load_fn;
    spawn_future_local(async move {
        while refresh_rx.changed().await.is_ok() {
            let mode = *refresh_state.view_mode_tx.borrow();
            clear_container(&refresh_container);
            refresh_scrolled.set_child(Some(&refresh_container));
            refresh_load(
                Arc::clone(&refresh_state),
                refresh_container.clone(),
                refresh_scrolled.clone(),
                mode,
            );
        }
    });

    scrolled
}

/// Remove all children from a `Box`.
fn clear_container(container: &Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

/// Switch the layout mode without re-creating card widgets.
///
/// Extracts existing card widgets from the current layout child
/// (`FlowBox` or `ListBox`), then arranges them in the new layout.
/// For `Column` mode, each card is wrapped in a `ListBoxRow`.
fn switch_layout(container: &Box, mode: ViewMode) {
    let cards = extract_cards(container);
    clear_container(container);

    match mode {
        Grid => populate_grid(container, "grid-layout", cards),
        Column => populate_list(container, "list-layout", cards),
    }
}

/// Extract card widgets from the current layout child of `container`.
///
/// Handles `FlowBox` (grid — direct children) and `ListBox` (column —
/// `ListBoxRow` children, unwrapping to get the card inside each row).
fn extract_cards(container: &Box) -> Vec<Widget> {
    let Some(layout) = container.first_child() else {
        return Vec::new();
    };

    if let Some(list) = layout.downcast_ref::<ListBox>() {
        let mut cards = Vec::new();
        let mut child = list.first_child();
        while let Some(row) = &child {
            cards.extend(row.first_child());
            child = row.next_sibling();
        }
        return cards;
    }

    let mut cards = Vec::new();
    let mut child = layout.first_child();
    while let Some(c) = &child {
        cards.push(c.clone());
        child = c.next_sibling();
    }
    cards
}

/// Populate a `FlowBox` in grid mode with pre-built card widgets.
pub fn populate_grid(container: &Box, tooltip: &str, cards: Vec<Widget>) {
    let flow = FlowBox::builder()
        .valign(Start)
        .halign(Center)
        .row_spacing(12)
        .column_spacing(12)
        .selection_mode(None)
        .can_focus(true)
        .tooltip_text(tooltip)
        .build();
    for card in cards {
        flow.append(&card);
    }
    container.append(&flow);
}

/// Populate a `ListBox` in column mode with pre-built card widgets.
pub fn populate_list(container: &Box, tooltip: &str, cards: Vec<Widget>) {
    let list = ListBox::builder()
        .selection_mode(None)
        .can_focus(true)
        .tooltip_text(tooltip)
        .css_classes(["boxed-list"])
        .build();
    for card in cards {
        let row = ListBoxRow::builder()
            .child(&card)
            .activatable(false)
            .build();
        list.append(&row);
    }
    container.append(&list);
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

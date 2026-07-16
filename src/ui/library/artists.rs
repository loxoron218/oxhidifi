//! Artist grid/column view.
//!
//! Displays artists in a responsive `FlowBox` grid or sortable
//! `GtkColumnView`. Both views are built once and held in a
//! `GtkStack` — switching between them toggles visibility without
//! any data re‑fetch or widget reconstruction.

use std::sync::Arc;

use {
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{
            Align::Start, Box, FlowBox, GestureClick, Image, Label, Orientation::Vertical, Overlay,
            Stack, Widget, accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, WidgetExt},
    },
    tracing::info,
};

use crate::{
    app::{AppState, NavigationEvent::ArtistDetail},
    storage::{
        Artist, Storage,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::library::{
        column_view::{NarrowState, build_artist_column_view},
        common::build_grid,
        empty::{
            EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
        },
    },
};

/// Size of artist avatar icons in pixels.
const AVATAR_SIZE: i32 = 180;

/// Number of artist cards to build per idle callback batch.
const GRID_BATCH_SIZE: usize = 10;

/// Build the artist grid view.
///
/// Creates a `LibraryGrid` that holds both grid (`FlowBox`) and column
/// (`ColumnView`) layouts in a `Stack`.  Data is fetched once; switching
/// between modes is a fast `set_visible_child_name` call.
///
/// # Arguments
///
/// * `state` - Application state
/// * `narrow_mode` - Narrow‑mode tracker for adaptive column hiding
pub fn build_artist_grid(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> LibraryGrid {
    let nm = Arc::clone(narrow_state);
    build_library_grid(state, &nm, |stack: &Stack, state, _, initial_mode| {
        let stack_clone = stack.clone();
        spawn_future_local(async move {
            populate_artist_views(&state, &stack_clone, initial_mode).await;
        });
    })
}

/// Fetch artist data and build **only the initial** view mode into `stack`.
///
/// Delegates to [`lazy_build_artist_mode`] which handles the fetch–
/// empty–build–set cycle.
async fn populate_artist_views(state: &Arc<AppState>, stack: &Stack, initial_mode: ViewMode) {
    lazy_build_artist_mode(state, stack, initial_mode).await;
}

/// Populate up to `GRID_BATCH_SIZE` artist cards into the flow box.
fn fill_artist_grid(artists: &mut Vec<Artist>, flow: &FlowBox, state: &Arc<AppState>) {
    for _ in 0..GRID_BATCH_SIZE {
        let Some(artist) = artists.pop() else { break };
        let card = build_artist_card(state, &artist);
        flow.append(&card.upcast::<Widget>());
    }
}

/// Check if the artists vec is exhausted and return the appropriate `ControlFlow`.
fn artist_done(artists: &[Artist]) -> ControlFlow {
    if artists.is_empty() { Break } else { Continue }
}

/// Build the given `mode` view (grid or column) and add it to `stack`.
///
/// Each mode is wrapped in its own `ScrolledWindow` so scroll positions
/// are kept independent.
fn build_artist_mode(state: &Arc<AppState>, stack: &Stack, mode: ViewMode, artists: &[Artist]) {
    match mode {
        Grid => {
            let grid_container = Box::builder().orientation(Vertical).build();
            let flow = build_grid("Artist library grid \u{2014} click an artist to view albums");
            grid_container.append(&flow);
            add_scrolled(stack, &grid_container, "grid");

            let state = Arc::clone(state);
            let mut artists: Vec<Artist> = artists.iter().rev().cloned().collect();

            idle_add_local(move || {
                fill_artist_grid(&mut artists, &flow, &state);
                artist_done(&artists)
            });
        }
        Column => {
            let column_view = build_artist_column_view(state, artists);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Lazily build a view mode that wasn't constructed at startup.
///
/// Re‑fetches data from storage, builds the requested `mode` widget,
/// adds it to `stack`, and switches to it.  No‑op if the child already
/// exists (race‑guard).
pub async fn lazy_build_artist_mode(state: &Arc<AppState>, stack: &Stack, mode: ViewMode) {
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    let artists = match state.storage.get_all_artists().await {
        Ok(a) => a
            .into_iter()
            .filter(|a| a.album_count > 0)
            .collect::<Vec<_>>(),
        Err(e) => {
            info!(error = %e, "Failed to load artists for lazy build");
            return;
        }
    };

    if artists.is_empty() {
        let empty_widget = build_empty_state(
            state,
            &EmptyStateParams {
                icon_name: "avatar-default-symbolic",
                icon_label: "Artist icon",
                heading: "No Artists Found",
                heading_label: "No artists found",
                description: "Add a music folder to see your artists here.",
                description_label: "Add a music folder to see your artists here.",
            },
        );
        stack.add_named(&empty_widget, Some("grid"));
        stack.set_visible_child_name("grid");
        return;
    }

    build_artist_mode(state, stack, mode, &artists);
    stack.set_visible_child_name(child_name);
}

/// Build the avatar widget for an artist.
///
/// Returns an `Image` with a generic artist icon.
fn build_artist_avatar() -> Widget {
    let avatar = Image::builder()
        .icon_name("avatar-default-symbolic")
        .pixel_size(AVATAR_SIZE / 2)
        .width_request(AVATAR_SIZE)
        .height_request(AVATAR_SIZE)
        .css_classes(["artist-avatar", "dim-label"])
        .build();
    avatar.update_property(&[PropertyLabel("Artist icon")]);
    avatar.upcast()
}

/// Build a single artist card widget.
///
/// Returns a `Box` containing a vertical layout with avatar,
/// name, and album count labels. Matches the album card structural
/// pattern (Overlay wrapper) for consistent card sizing.
fn build_artist_card(state: &Arc<AppState>, artist: &Artist) -> Box {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .tooltip_text(format!("View albums by {}", artist.name))
        .build();

    let avatar = build_artist_avatar();

    let overlay = Overlay::new();
    overlay.set_child(Some(&avatar));
    overlay.set_css_classes(&["cover-overlay"]);

    card.append(&overlay.upcast::<Widget>());

    let name_label = Label::builder()
        .label(&artist.name)
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();
    name_label.update_property(&[PropertyLabel(&format!("Artist: {}", artist.name))]);

    let album_count_label = Label::builder()
        .label(format!("{} albums", artist.album_count))
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    album_count_label.update_property(&[PropertyLabel(&format!(
        "{} albums by {}",
        artist.album_count, artist.name
    ))]);

    card.append(&name_label);
    card.append(&album_count_label);

    let gesture = GestureClick::new();
    let state_clone = Arc::clone(state);
    let artist_id = artist.id;
    gesture.connect_released(move |_, _, _, _| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            state.send_navigation_event(ArtistDetail(artist_id)).await;
        });
    });
    card.add_controller(gesture);

    card
}

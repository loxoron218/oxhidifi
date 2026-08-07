//! Artist grid/column view.
//!
//! Displays artists in a responsive `FlowBox` grid or sortable
//! `GtkColumnView`. Both views are built once and held in a
//! `GtkStack` — switching between them toggles visibility without
//! any data re‑fetch or widget reconstruction.

use std::{
    cmp::Ordering::{self, Equal},
    sync::{Arc, atomic::Ordering::Relaxed},
};

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
    tracing::warn,
};

use crate::{
    app::{AppState, CachedArtistData, NavigationEvent::ArtistDetail},
    storage::{
        Storage,
        records::Artist,
        settings::{
            ActiveTab::Artists,
            ViewMode::{self, Column, Grid},
        },
        sort_rules::{
            ArtistSortCriteria::{AlbumCount, Name},
            ArtistSortItem,
            SortOrder::Descending,
        },
    },
    ui::library::{
        column_view::build_artist_column_view,
        common::{
            RebuildAction::{DeferDirty, Resize},
            build_grid, clear_mode_children, decide_rebuild, fill_grid_batch,
            memoized_sort_indices, resize_grid_batched, setup_flowbox_keyboard_nav,
            spawn_listen_sort_zoom, take_stale_mode_child, try_build_from_cache,
        },
        empty::{
            EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
        },
        narrow_state::NarrowState,
    },
    zoom::grid_cover_size,
};

/// Compare two artists by a single sort item, applying the configured order.
fn cmp_artists(a: &Artist, b: &Artist, item: &ArtistSortItem) -> Ordering {
    let mut cmp = match item.criteria {
        Name => a.name.cmp(&b.name),
        AlbumCount => a.album_count.cmp(&b.album_count),
    };
    if item.order == Descending {
        cmp = cmp.reverse();
    }
    cmp
}

/// Compute a display order for `artists` as a sorted index vector.
///
/// Single‑pass `sort_unstable_by` over indices — each criteria is checked
/// in priority order until a non‑equal comparison is found. Borrows the
/// artist data so rebuilds never deep‑clone the `Vec<Artist>`.
fn sorted_artist_indices(artists: &[Artist], sort_items: &[ArtistSortItem]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..artists.len()).collect();
    indices.sort_unstable_by(|&a, &b| {
        sort_items
            .iter()
            .find_map(|item| {
                let cmp = cmp_artists(&artists[a], &artists[b], item);
                (cmp != Equal).then_some(cmp)
            })
            .unwrap_or(Equal)
    });
    indices
}

/// Memoized display order for the cached artist data.
///
/// Returns the sort indices from the grid's memo when the library generation
/// and the current sort configuration both match, so repeated tab/mode
/// switches reuse the sort instead of re-comparing every artist. Computes,
/// caches, and returns them otherwise.
fn artist_sort_indices(state: &Arc<AppState>, cached: &CachedArtistData) -> Arc<[usize]> {
    let generation = state.artist_grid.generation.load(Relaxed);
    let config = state.storage.get_artists_sort();
    memoized_sort_indices(generation, config, &state.artist_grid.memo, |config| {
        sorted_artist_indices(&cached.artists, config)
    })
}

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
    let populate =
        |stack: &Stack, state: Arc<AppState>, _: Arc<NarrowState>, initial_mode: ViewMode| {
            let sc = stack.clone();
            spawn_future_local(async move {
                lazy_build_artist_mode(&state, &sc, initial_mode).await;
            });
        };
    let lg = build_library_grid(state, &nm, populate);

    let grid_state = Arc::clone(state);
    let grid_stack = lg.mode_stack.clone();
    let preview_state = Arc::clone(state);
    let preview_stack = lg.mode_stack.clone();
    spawn_listen_sort_zoom(
        state,
        state.artists_sort_rx.clone(),
        state.artists_zoom_rx.clone(),
        move || {
            if *preview_state.active_tab_tx.borrow() == Artists
                && *preview_state.view_mode_tx.borrow() == Grid
                && preview_state.artist_grid.ready.load(Relaxed)
            {
                resize_artist_grid(&preview_state, &preview_stack);
            }
        },
        async move |sort_fired, zoom_fired| {
            rebuild_artist_current_mode(&grid_state, &grid_stack, sort_fired, zoom_fired).await;
        },
    );

    lg
}

/// Rebuild or resize the artist grid after a sort/zoom change.
///
/// Zoom-only changes resize the existing cards in place (no widget churn,
/// scroll position preserved). Sort changes — or any state that invalidates
/// the current cards — fall back to a full rebuild from the in-memory cache.
/// When the tab is hidden, marks the grid dirty so it is rebuilt on switch.
async fn rebuild_artist_current_mode(
    state: &Arc<AppState>,
    mode_stack: &Stack,
    sort_fired: bool,
    zoom_fired: bool,
) {
    let action = decide_rebuild(
        *state.active_tab_tx.borrow(),
        Artists,
        *state.view_mode_tx.borrow(),
        sort_fired,
        zoom_fired,
        state.artist_grid.ready.load(Relaxed),
    );
    if action == DeferDirty {
        state.artist_grid.dirty.store(true, Relaxed);
        return;
    }
    if action == Resize && resize_artist_grid(state, mode_stack) {
        return;
    }
    let Some(mode) = take_stale_mode_child(state, Artists, mode_stack) else {
        state.artist_grid.dirty.store(true, Relaxed);
        return;
    };
    if sort_fired {
        clear_mode_children(mode_stack);
    }
    state.artist_grid.dirty.store(false, Relaxed);
    lazy_build_artist_mode(state, mode_stack, mode).await;
}

/// Resize the live artist grid's cards to the current zoom level in place.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) that
/// updates the `FlowBox` spacing and every card's avatar size across idle
/// callbacks, so a zoom on a large library does not block the frame clock.
/// Artists have no per-size cover art to decode, so no cover dispatch.
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
fn resize_artist_grid(state: &Arc<AppState>, mode_stack: &Stack) -> bool {
    let build_seq = state.artist_grid.build_seq.load(Relaxed);
    let stale_state = Arc::clone(state);
    resize_grid_batched(
        state,
        mode_stack,
        move || stale_state.artist_grid.build_seq.load(Relaxed) != build_seq,
        |_, _, _| {},
        |_, _| {},
    )
}

/// Populate a batch of artist cards into the flow box.
///
/// Bails out if `build_seq` no longer matches the current build generation
/// (the build was superseded by a rebuild or refresh). On the final batch,
/// marks the grid ready for in-place zoom resizing. Returns `Break` when
/// done or stale.
fn fill_artist_grid(
    cached: &CachedArtistData,
    remaining: &mut Vec<usize>,
    overlays: &mut Vec<Overlay>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    build_seq: u64,
) -> ControlFlow {
    if fill_grid_batch(
        state.artist_grid.build_seq.load(Relaxed) == build_seq,
        state,
        remaining,
        |idx, size| {
            let (card, overlay) = build_artist_card(state, &cached.artists[idx], size);
            overlays.push(overlay);
            flow.append(&card.upcast::<Widget>());
        },
    ) == Continue
    {
        return Continue;
    }
    if state.artist_grid.build_seq.load(Relaxed) != build_seq {
        return Break;
    }
    state.artist_grid.ready.store(true, Relaxed);
    Break
}

/// Build the given `mode` view (grid or column) and add it to `stack`.
///
/// Borrows the shared artist data from `cached` and derives the display order
/// from `indices`, so no deep clone is performed. Each mode is wrapped in
/// its own `ScrolledWindow` so scroll positions are kept independent.
///
/// # Arguments
///
/// * `cached` - Shared artist data
/// * `indices` - Display order of `cached.artists` (e.g. a sorted index vector)
fn build_artist_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    mode: ViewMode,
    cached: &CachedArtistData,
    indices: &[usize],
) {
    let cover_size = grid_cover_size(state.storage.get_grid_zoom_level());
    match mode {
        Grid => {
            state.artist_grid.ready.store(false, Relaxed);
            let grid_container = Box::builder().orientation(Vertical).build();
            let flow = build_grid(
                "Artist library grid \u{2014} click an artist to view albums",
                cover_size,
            );
            grid_container.append(&flow);
            add_scrolled(stack, &grid_container, "grid");

            let artist_ids: Vec<i64> = indices.iter().map(|&i| cached.artists[i].id).collect();
            setup_flowbox_keyboard_nav(&flow, state, artist_ids, ArtistDetail);

            let cached_owned = CachedArtistData {
                artists: Arc::clone(&cached.artists),
            };
            let state = Arc::clone(state);
            let build_seq = state
                .artist_grid
                .build_seq
                .fetch_add(1, Relaxed)
                .wrapping_add(1);
            let mut overlays: Vec<Overlay> = Vec::new();
            let mut remaining: Vec<usize> = indices.iter().rev().copied().collect();

            idle_add_local(move || {
                fill_artist_grid(
                    &cached_owned,
                    &mut remaining,
                    &mut overlays,
                    &flow,
                    &state,
                    build_seq,
                )
            });
        }
        Column => {
            let column_view = build_artist_column_view(state, cached, indices);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Show the empty‑artists state widget, replacing any existing grid child.
fn show_artists_empty(state: &Arc<AppState>, stack: &Stack) {
    state.artist_grid.ready.store(false, Relaxed);
    let empty = build_empty_state(
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
    if stack.child_by_name("grid").is_none() {
        stack.add_named(&empty, Some("grid"));
    }
    stack.set_visible_child_name("grid");
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

    if try_build_from_cache(
        &state.artist_grid.cache,
        stack,
        child_name,
        |cached| cached.artists.is_empty(),
        |cached| {
            let indices = artist_sort_indices(state, cached);
            build_artist_mode(state, stack, mode, cached, &indices);
        },
        || show_artists_empty(state, stack),
    ) {
        return;
    }

    let my_gen = state.artist_grid.generation.load(Relaxed);
    let artists = match state.storage.get_all_artists().await {
        Ok(a) => a
            .into_iter()
            .filter(|a| a.album_count > 0)
            .collect::<Vec<_>>(),
        Err(e) => {
            warn!(error = %e, "Failed to load artists for lazy build");
            return;
        }
    };

    if state.artist_grid.generation.load(Relaxed) != my_gen {
        return;
    }
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    if artists.is_empty() {
        *state.artist_grid.cache.lock() = Some(CachedArtistData {
            artists: Arc::new(artists),
        });
        show_artists_empty(state, stack);
        return;
    }

    let cached = CachedArtistData {
        artists: Arc::new(artists),
    };
    let indices = artist_sort_indices(state, &cached);
    build_artist_mode(state, stack, mode, &cached, &indices);
    *state.artist_grid.cache.lock() = Some(cached);
    stack.set_visible_child_name(child_name);
}

/// Build the avatar widget for an artist.
///
/// Returns an `Image` with a generic artist icon.
fn build_artist_avatar(size: i32) -> Widget {
    let avatar = Image::builder()
        .icon_name("avatar-default-symbolic")
        .pixel_size(size / 2)
        .width_request(size)
        .height_request(size)
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
///
/// Also returns the `Overlay` wrapping the avatar so zoom can resize it
/// in place without rebuilding the card.
fn build_artist_card(state: &Arc<AppState>, artist: &Artist, size: i32) -> (Box, Overlay) {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .tooltip_text(format!("View albums by {}", artist.name))
        .build();
    card.update_property(&[PropertyLabel(&format!("View albums by {}", artist.name))]);

    let avatar = build_artist_avatar(size);

    let overlay = Overlay::new();
    overlay.set_child(Some(&avatar));
    overlay.set_css_classes(&["cover-overlay"]);

    card.append(&overlay.clone().upcast::<Widget>());

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

    (card, overlay)
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::sort_rules::{
            ArtistSortCriteria::{AlbumCount, Name},
            ArtistSortItem,
            SortOrder::{Ascending, Descending},
        },
        ui::library::artists::{Artist, sorted_artist_indices},
    };

    fn mock_artist(id: i64, name: &str, album_count: i32) -> Artist {
        Artist {
            id,
            name: name.into(),
            album_count,
        }
    }

    #[test]
    fn sorted_artist_indices_orders_by_priority_items() {
        let artists = vec![
            mock_artist(1, "Beta", 2),
            mock_artist(2, "Alpha", 5),
            mock_artist(3, "Gamma", 5),
        ];

        let items = vec![
            ArtistSortItem {
                criteria: AlbumCount,
                order: Descending,
            },
            ArtistSortItem {
                criteria: Name,
                order: Ascending,
            },
        ];

        let indices = sorted_artist_indices(&artists, &items);
        assert_eq!(indices, vec![1, 2, 0]);
    }

    #[test]
    fn sorted_artist_indices_orders_by_name() {
        let artists = vec![mock_artist(1, "Beta", 1), mock_artist(2, "Alpha", 1)];

        let items = vec![ArtistSortItem {
            criteria: Name,
            order: Ascending,
        }];

        let indices = sorted_artist_indices(&artists, &items);
        assert_eq!(indices, vec![1, 0]);
    }
}

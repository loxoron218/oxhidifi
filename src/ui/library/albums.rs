//! Album grid/column view.
//!
//! Displays albums in a responsive `FlowBox` grid (grid mode) or a
//! sortable `GtkColumnView` (column mode). Only the *initial* mode is
//! built at startup; the other mode is lazily built on first switch.

use std::{collections::HashMap, sync::Arc};

use {
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
            prelude::Cast,
            spawn_future_local,
        },
        gtk::{Box, FlowBox, Orientation::Vertical, Overlay, Stack, Widget},
        prelude::BoxExt,
    },
    tokio::join,
    tracing::warn,
};

use crate::{
    app::{AppState, NavigationEvent::AlbumDetail},
    storage::{
        Storage,
        formats::FormatInfo,
        records::Album,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::{
        CoverArtCache,
        library::{
            album_card::{build_album_card, load_cover_art_async},
            column_view::build_album_column_view,
            common::{build_grid, setup_flowbox_keyboard_nav},
            empty::{
                EmptyStateParams, LibraryGrid, add_scrolled, build_empty_state, build_library_grid,
            },
            narrow_state::NarrowState,
        },
    },
};

/// Number of album cards to build per idle callback batch.
const GRID_BATCH_SIZE: usize = 10;

/// Build the album grid view.
///
/// Creates a `LibraryGrid` that holds both grid (`FlowBox`) and column
/// (`ColumnView`) layouts in a `Stack`.  Data is fetched once; switching
/// between modes is a fast `set_visible_child_name` call.
///
/// # Arguments
///
/// * `state` - Application state
/// * `narrow_mode` - Narrow‑mode tracker for adaptive column hiding
pub fn build_album_grid(state: &Arc<AppState>, narrow_state: &Arc<NarrowState>) -> LibraryGrid {
    let nm = Arc::clone(narrow_state);
    build_library_grid(
        state,
        &nm,
        |stack: &Stack, state, narrow_state, initial_mode| {
            let stack_clone = stack.clone();
            spawn_future_local(async move {
                populate_album_views(&state, &stack_clone, &narrow_state, initial_mode).await;
            });
        },
    )
}

/// Fetch album data and build **only the initial** view mode into `stack`.
///
/// Delegates to [`lazy_build_album_mode`] which handles the fetch–
/// empty–build–set cycle.
async fn populate_album_views(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    initial_mode: ViewMode,
) {
    lazy_build_album_mode(state, stack, narrow_state, initial_mode).await;
}

/// Populate up to `GRID_BATCH_SIZE` album cards into the flow box.
/// Returns `Continue` if more items remain, `Break` when done.
fn fill_album_grid(
    snapshots: &mut Vec<(Album, String, FormatInfo)>,
    overlays: &mut Vec<Overlay>,
    cover_art_data: &mut Vec<(i64, usize, String)>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    cache: &Arc<CoverArtCache>,
) {
    for _ in 0..GRID_BATCH_SIZE {
        let Some((album, artist_name, fi)) = snapshots.pop() else {
            break;
        };
        let index = overlays.len();
        let (card, overlay) = build_album_card(state, &album, &artist_name, &fi);
        if let Some(path) = &album.artwork_path {
            cover_art_data.push((album.id, index, path.clone()));
        }
        overlays.push(overlay);
        flow.append(&card.upcast::<Widget>());
    }
    if snapshots.is_empty() {
        load_cover_art_async(cover_art_data, overlays, cache);
    }
}

/// Check if the snapshots are exhausted and return the appropriate `ControlFlow`.
fn check_done(snapshots: &[(Album, String, FormatInfo)]) -> ControlFlow {
    if snapshots.is_empty() {
        Break
    } else {
        Continue
    }
}

/// Build the given `mode` view (grid or column) and add it to `stack`.
///
/// Each mode is wrapped in its own `ScrolledWindow` so scroll positions
/// are kept independent.  The other mode is NOT built here — it will be
/// lazily built on first toggle via [`lazy_build_album_mode`].
fn build_album_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &NarrowState,
    mode: ViewMode,
    albums: &[Album],
    artist_names: &HashMap<i64, String>,
    format_info: &HashMap<i64, FormatInfo>,
) {
    match mode {
        Grid => {
            let grid_container = Box::builder().orientation(Vertical).build();
            let flow = build_grid("Album library grid \u{2014} click an album to play");
            grid_container.append(&flow);
            add_scrolled(stack, &grid_container, "grid");

            let album_ids: Vec<i64> = albums.iter().map(|a| a.id).collect();
            setup_flowbox_keyboard_nav(&flow, state, album_ids, AlbumDetail);

            let state = Arc::clone(state);
            let cache = Arc::clone(&state.cover_art_cache);

            let mut snapshots: Vec<(Album, String, FormatInfo)> = albums
                .iter()
                .rev()
                .map(|album| {
                    let artist_name = artist_names
                        .get(&album.artist_id)
                        .map_or_else(|| "Unknown Artist".to_string(), Clone::clone);
                    let fi = format_info.get(&album.id).cloned().unwrap_or_default();
                    (album.clone(), artist_name, fi)
                })
                .collect();

            let mut overlays: Vec<Overlay> = Vec::new();
            let mut cover_art_data: Vec<(i64, usize, String)> = Vec::new();

            idle_add_local(move || {
                fill_album_grid(
                    &mut snapshots,
                    &mut overlays,
                    &mut cover_art_data,
                    &flow,
                    &state,
                    &cache,
                );
                check_done(&snapshots)
            });
        }
        Column => {
            let column_view =
                build_album_column_view(state, albums, artist_names, narrow_state, format_info);
            add_scrolled(stack, &column_view, "column");
        }
    }
}

/// Lazily build a view mode that wasn't constructed at startup.
///
/// Re‑fetches data from storage, builds the requested `mode` widget,
/// adds it to `stack`, and switches to it.  This is a no‑op if the
/// child already exists (race‑guard).
pub async fn lazy_build_album_mode(
    state: &Arc<AppState>,
    stack: &Stack,
    narrow_state: &Arc<NarrowState>,
    mode: ViewMode,
) {
    let child_name = match mode {
        Grid => "grid",
        Column => "column",
    };
    if stack.child_by_name(child_name).is_some() {
        stack.set_visible_child_name(child_name);
        return;
    }

    let (albums_res, artist_names_res) = join!(
        state.storage.get_all_albums(),
        state.storage.get_all_artists(),
    );

    let albums = match albums_res {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, "Failed to load albums for lazy build");
            return;
        }
    };

    if albums.is_empty() {
        let empty_widget = build_empty_state(
            state,
            &EmptyStateParams {
                icon_name: "folder-music-symbolic",
                icon_label: "Music library icon",
                heading: "No Albums Found",
                heading_label: "No albums found",
                description: "Add a music folder to see your albums here.",
                description_label: "Add a music folder to see your albums here.",
            },
        );
        stack.add_named(&empty_widget, Some("grid"));
        stack.set_visible_child_name("grid");
        return;
    }

    let album_ids: Vec<i64> = albums.iter().map(|a| a.id).collect();
    let format_info = state
        .storage
        .get_albums_format_info(&album_ids)
        .await
        .unwrap_or_default();

    let artist_names: HashMap<i64, String> = match artist_names_res {
        Ok(artists) => artists.into_iter().map(|a| (a.id, a.name)).collect(),
        Err(e) => {
            warn!(error = %e, "Failed to load artists for lazy build");
            HashMap::new()
        }
    };

    build_album_mode(
        state,
        stack,
        narrow_state,
        mode,
        &albums,
        &artist_names,
        &format_info,
    );
    stack.set_visible_child_name(child_name);
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::ControlFlow::{Break, Continue},
            gtk::{self, test},
        },
    };

    use crate::{
        storage::{formats::FormatInfo, records::Album},
        ui::library::albums::check_done,
    };

    fn mock_album(id: i64) -> Album {
        Album {
            id,
            title: "Album".into(),
            artist_id: 1,
            year: Some(2020),
            genre: None,
            artwork_path: None,
            track_count: 10,
            total_duration: 300.0,
            format_summary: String::new(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96000),
        }
    }

    #[test]
    fn check_done_breaks_when_snapshots_exhausted() -> Result<()> {
        let snapshots: Vec<(Album, String, FormatInfo)> = Vec::new();
        ensure!(check_done(&snapshots) == Break);
        Ok(())
    }

    #[test]
    fn check_done_continues_with_remaining_snapshots() -> Result<()> {
        let snapshots = vec![(mock_album(1), "Artist".into(), FormatInfo::default())];
        ensure!(check_done(&snapshots) == Continue);
        Ok(())
    }
}

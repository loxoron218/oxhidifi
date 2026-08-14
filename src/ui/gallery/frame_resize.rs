//! Album cover frame resize and grid cover population for in-place zoom.

use std::{
    collections::HashMap,
    mem::take,
    sync::{Arc, atomic::Ordering::Relaxed},
};

use libadwaita::{
    glib::{
        ControlFlow::{self, Break, Continue},
        prelude::Cast,
    },
    gtk::{FlowBox, Overlay, Stack, Widget},
};

use crate::{
    app::runtime::{AppState, CachedAlbumData},
    ui::{
        gallery::{
            card::{build_album_card, load_cover_art_async, resolve_cover_widget},
            grid_flow::{fill_grid_batch, resize_grid_batched},
        },
        texture_pool::CoverArtCache,
        zoom::grid_cover_size,
    },
};

/// Snapshot of the album grid's cover state captured at resize time.
struct AlbumResizeSnapshot {
    /// Build sequence captured when the snapshot was taken.
    build_seq: u64,
    /// Reverse map: overlay index → album ID.
    idx_to_album: Arc<HashMap<usize, i64>>,
    /// Shared decoded-cover cache.
    cover_cache: Arc<CoverArtCache>,
    /// App state clone for the stale-build predicate.
    state: Arc<AppState>,
}

/// Populate a batch of album cards into the flow box.
///
/// Bails out if `build_seq` no longer matches the current build generation
/// (the build was superseded by a rebuild or refresh). On the final batch,
/// dispatches cover decoding, snapshots the cover metadata for in-place zoom
/// resizing, and marks the grid ready. Returns `Break` when done or stale.
pub fn fill_album_grid(
    cached: &CachedAlbumData,
    remaining: &mut Vec<usize>,
    overlays: &mut Vec<Overlay>,
    cover_art_data: &mut Vec<(i64, usize, String)>,
    flow: &FlowBox,
    state: &Arc<AppState>,
    build_seq: u64,
) -> ControlFlow {
    if fill_grid_batch(
        state.album_grid.build_seq.load(Relaxed) == build_seq,
        state,
        remaining,
        |idx, size| {
            let Some(album) = cached.albums.get(idx) else {
                return;
            };
            let index = overlays.len();
            let artist_name = cached
                .artist_names
                .get(&album.artist_id)
                .map_or("Unknown Artist", String::as_str);
            let fi = cached
                .format_info
                .get(&album.id)
                .cloned()
                .unwrap_or_default();
            let (card, overlay) = build_album_card(state, album, artist_name, &fi, size);
            if let Some(path) = &album.artwork_path {
                cover_art_data.push((album.id, index, path.clone()));
            }
            overlays.push(overlay);
            flow.append(&card.upcast::<Widget>());
        },
    ) == Continue
    {
        return Continue;
    }
    if state.album_grid.build_seq.load(Relaxed) != build_seq {
        return Break;
    }
    let size = grid_cover_size(state.storage.get_grid_zoom_level());
    load_cover_art_async(
        state,
        cover_art_data,
        overlays,
        &state.cover_art_cache,
        size,
    );
    *state.album_grid_covers.lock() = Arc::new(take(cover_art_data));
    state.album_grid.ready.store(true, Relaxed);
    Break
}

/// Snapshot the album grid's cover state for an in-place zoom resize.
///
/// Reads the build sequence, the cover data (`album ID`, `overlay index`,
/// `artwork path`) to derive the reverse map (overlay index → album ID), the
/// shared cover cache, and the app state into a single snapshot. Shared by
/// the preview and full resize paths so both resize the same live grid
/// without re-reading shared state.
fn album_resize_snapshot(state: &Arc<AppState>) -> AlbumResizeSnapshot {
    let build_seq = state.album_grid.build_seq.load(Relaxed);
    let cover_data = Arc::clone(&*state.album_grid_covers.lock());
    let idx_to_album: Arc<HashMap<usize, i64>> =
        Arc::new(cover_data.iter().map(|&(aid, idx, _)| (idx, aid)).collect());
    AlbumResizeSnapshot {
        build_seq,
        idx_to_album,
        cover_cache: Arc::clone(&state.cover_art_cache),
        state: Arc::clone(state),
    }
}

/// Build the per-card cover-resolve closure for a batched zoom resize.
///
/// Clones the reverse map and cover cache into a `'static` closure that
/// resolves each card's cover to the new size or a cached texture — without
/// dispatching new decode requests.
fn album_cover_resolver(
    idx_to_album: &Arc<HashMap<usize, i64>>,
    cover_cache: &Arc<CoverArtCache>,
) -> impl FnMut(usize, &Overlay, i32) + 'static {
    let idx_to_album = Arc::clone(idx_to_album);
    let cover_cache = Arc::clone(cover_cache);
    move |idx: usize, overlay: &Overlay, size: i32| {
        resolve_cover_at(&idx_to_album, &cover_cache, idx, overlay, size);
    }
}

/// Resize the live album grid's cards and resolve their cover widgets.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) that
/// resizes every card's cover to the current zoom level and resolves each
/// to its cached texture or a placeholder — without dispatching new decode
/// requests. Shared by the zoom preview (instant feedback without per-click
/// decode waves) and [`resize_album_grid`] (which adds the debounced decode
/// dispatch after the traversal completes).
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
pub fn apply_album_resize(state: &Arc<AppState>, mode_stack: &Stack) -> bool {
    let snapshot = album_resize_snapshot(state);
    let resolve_cards = album_cover_resolver(&snapshot.idx_to_album, &snapshot.cover_cache);
    let stale_state = Arc::clone(&snapshot.state);
    resize_grid_batched(
        state,
        mode_stack,
        move || stale_state.album_grid.build_seq.load(Relaxed) != snapshot.build_seq,
        resolve_cards,
        |_, _| {},
    )
}

/// Resize the live album grid's cards and dispatch cover decoding.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) and,
/// once the traversal completes, dispatches cover decoding for the new size
/// for any art not yet cached. The cover metadata (album ID → artwork path,
/// in card order) is read from state.
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
pub fn resize_album_grid(state: &Arc<AppState>, mode_stack: &Stack) -> bool {
    let snapshot = album_resize_snapshot(state);
    let resolve_cards = album_cover_resolver(&snapshot.idx_to_album, &snapshot.cover_cache);
    let dispatch_covers = {
        let state = Arc::clone(state);
        let cover_data = Arc::clone(&*state.album_grid_covers.lock());
        let cover_cache = Arc::clone(&snapshot.cover_cache);
        move |overlays: Vec<Overlay>, size| {
            load_cover_art_async(&state, &cover_data, &overlays, &cover_cache, size);
        }
    };
    let stale_state = Arc::clone(&snapshot.state);
    resize_grid_batched(
        state,
        mode_stack,
        move || stale_state.album_grid.build_seq.load(Relaxed) != snapshot.build_seq,
        resolve_cards,
        dispatch_covers,
    )
}

/// Resolve a single card's cover from the grid's cover snapshot, if it has
/// one. Applied during a batched zoom resize after the card's geometry pass.
fn resolve_cover_at(
    idx_to_album: &HashMap<usize, i64>,
    cover_cache: &CoverArtCache,
    idx: usize,
    overlay: &Overlay,
    size: i32,
) {
    if let Some(&album_id) = idx_to_album.get(&idx) {
        resolve_cover_widget(cover_cache, overlay, album_id, size);
    }
}

//! Album cover frame resize and grid cover population for in-place zoom.

use std::{
    collections::HashMap,
    mem::take,
    sync::{Arc, atomic::Ordering::Relaxed},
};

use libadwaita::{
    glib::ControlFlow::{self, Break, Continue},
    gtk::{FlowBox, Overlay, Stack, Widget},
    prelude::Cast,
};

use crate::{
    app::runtime::{AppState, CachedAlbumData},
    ui::{
        gallery::{
            card::build_album_card,
            cover_dispatch::{load_cover_art_async, resolve_cover_at},
            grid_batch::{fill_grid_batch, resize_grid_batched},
            label_sizing::resize_album_card,
        },
        texture_pool::CoverArtCache,
    },
};

/// Per-build inputs for the album grid's batched idle fill.
///
/// Groups the arguments that stay constant across batches so
/// [`fill_album_grid`] stays focused on the mutable batch cursors.
#[derive(Debug)]
pub struct AlbumFillContext<'a> {
    /// Flow box receiving the built cards.
    pub flow: &'a FlowBox,
    /// Application state (build-sequence source, cover dispatch).
    pub state: &'a Arc<AppState>,
    /// Build sequence captured at schedule time for staleness checks.
    pub build_seq: u64,
    /// Cover size to build cards and decode covers at (already resolved for
    /// narrow windows by the caller).
    pub cover_size: i32,
}

/// Snapshot of the album grid's cover state captured at resize time.
struct AlbumResizeSnapshot {
    /// Build sequence captured when the snapshot was taken.
    build_seq: u64,
    /// Zoom sequence captured when the snapshot was taken.
    zoom_seq: u64,
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
    ctx: &AlbumFillContext<'_>,
) -> ControlFlow {
    if fill_grid_batch(
        ctx.state.album_grid.build_seq.load(Relaxed) == ctx.build_seq,
        ctx.cover_size,
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
            let (card, overlay) = build_album_card(ctx.state, album, artist_name, &fi, size);
            if let Some(path) = &album.artwork_path {
                cover_art_data.push((album.id, index, path.clone()));
            }
            overlays.push(overlay);
            ctx.flow.append(&card.upcast::<Widget>());
        },
    ) == Continue
    {
        return Continue;
    }
    if ctx.state.album_grid.build_seq.load(Relaxed) != ctx.build_seq {
        return Break;
    }
    load_cover_art_async(
        ctx.state,
        cover_art_data,
        overlays,
        &ctx.state.cover_art_cache,
        ctx.cover_size,
    );
    *ctx.state.album_grid_covers.lock() = Arc::new(take(cover_art_data));
    ctx.state.album_grid.ready.store(true, Relaxed);
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
    let zoom_seq = state.album_grid.zoom_seq.load(Relaxed);
    let cover_data = Arc::clone(&*state.album_grid_covers.lock());
    let idx_to_album: Arc<HashMap<usize, i64>> =
        Arc::new(cover_data.iter().map(|&(aid, idx, _)| (idx, aid)).collect());
    AlbumResizeSnapshot {
        build_seq,
        zoom_seq,
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
        resize_album_card(overlay, size);
        resolve_cover_at(&idx_to_album, &cover_cache, idx, overlay, size);
    }
}

/// Resize the live album grid's cards and resolve their cover widgets.
///
/// Schedules a batched in-place resize (see [`resize_grid_batched`]) that
/// resizes every card's cover to `cover_size` and resolves each
/// to its cached texture or a placeholder — without dispatching new decode
/// requests. Shared by the zoom preview (instant feedback without per-click
/// decode waves) and [`resize_album_grid`] (which adds the debounced decode
/// dispatch after the traversal completes).
///
/// # Arguments
///
/// * `cover_size` - Cover size to resize to (already resolved for narrow windows by the caller).
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
pub fn apply_album_resize(state: &Arc<AppState>, mode_stack: &Stack, cover_size: i32) -> bool {
    let snapshot = album_resize_snapshot(state);
    let resolve_cards = album_cover_resolver(&snapshot.idx_to_album, &snapshot.cover_cache);
    let is_stale = album_resize_stale_predicate(&snapshot);
    resize_grid_batched(
        state,
        mode_stack,
        cover_size,
        is_stale,
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
/// # Arguments
///
/// * `cover_size` - Cover size to resize to (already resolved for narrow windows by the caller).
///
/// # Returns
///
/// `true` when the grid was located and a batch scheduled, `false` when the
/// `FlowBox` could not be located (caller falls back to a full rebuild).
pub fn resize_album_grid(state: &Arc<AppState>, mode_stack: &Stack, cover_size: i32) -> bool {
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
    let is_stale = album_resize_stale_predicate(&snapshot);
    resize_grid_batched(
        state,
        mode_stack,
        cover_size,
        is_stale,
        resolve_cards,
        dispatch_covers,
    )
}

/// Build the staleness predicate for a batched album resize.
///
/// Bails when a newer build or zoom superseded the snapshot, so a preview
/// traversal aborts instead of fighting the debounced resize for the same
/// `FlowBox`.
fn album_resize_stale_predicate(snapshot: &AlbumResizeSnapshot) -> impl Fn() -> bool + use<> {
    let state = Arc::clone(&snapshot.state);
    let build_seq = snapshot.build_seq;
    let zoom_seq = snapshot.zoom_seq;
    move || {
        state.album_grid.build_seq.load(Relaxed) != build_seq
            || state.album_grid.zoom_seq.load(Relaxed) != zoom_seq
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gdk::MemoryFormat::R8g8b8a8,
            gtk::{self, Orientation::Horizontal, Picture, test},
            prelude::{Cast, WidgetExt},
        },
        tempfile::NamedTempFile,
    };

    use crate::{
        app::runtime::AppState,
        storage::{catalog::Album, formats::FormatInfo},
        ui::{
            gallery::{
                card::build_album_card,
                cover_dispatch::{album_covers_ready, resolve_cover_widget},
                grid_flow::resize_overlay_cover,
                label_sizing::resize_album_card,
            },
            image_decode::{DecodedCover, raw_to_texture},
        },
    };

    fn covers_state(entries: Vec<(i64, usize, String)>) -> Result<AppState> {
        let state = AppState::mock()?;
        *state.album_grid_covers.lock() = Arc::new(entries);
        Ok(state)
    }

    fn sized_cover(size: i32) -> DecodedCover {
        DecodedCover {
            width: size,
            height: size,
            rowstride: usize::try_from(size).unwrap_or(0).saturating_mul(4),
            format: R8g8b8a8,
            data: vec![
                0;
                usize::try_from(size)
                    .unwrap_or(0)
                    .saturating_mul(usize::try_from(size).unwrap_or(0))
                    .saturating_mul(4)
            ],
        }
    }

    #[test]
    fn covers_ready_when_no_artwork() -> Result<()> {
        let state = covers_state(Vec::new())?;
        ensure!(
            album_covers_ready(&state, 180),
            "an empty cover snapshot must report ready"
        );
        Ok(())
    }

    #[test]
    fn covers_ready_when_all_cached() -> Result<()> {
        let cover = NamedTempFile::new()?;
        let path = cover.path().to_string_lossy().to_string();
        let state = covers_state(vec![(1, 0, path)])?;
        state
            .cover_art_cache
            .insert(1, 180, raw_to_texture(&sized_cover(180)));
        ensure!(
            album_covers_ready(&state, 180),
            "an exact cached size must report ready"
        );
        ensure!(
            !album_covers_ready(&state, 150),
            "a missing size with an existing file must report not ready"
        );
        Ok(())
    }

    #[test]
    fn covers_ready_ignores_missing_files() -> Result<()> {
        let state = covers_state(vec![(1, 0, "/nonexistent-oxhidifi-cover.jpg".to_string())])?;
        ensure!(
            album_covers_ready(&state, 180),
            "a missing file must not block the fast path"
        );
        Ok(())
    }

    fn zoomed_card_natural_width(
        state: &Arc<AppState>,
        cached_sizes: &[i32],
        from_size: i32,
        to_size: i32,
    ) -> (i32, bool) {
        for size in cached_sizes {
            state
                .cover_art_cache
                .insert(7, *size, raw_to_texture(&sized_cover(*size)));
        }
        let album = Album {
            id: 7,
            title: "Test Album".into(),
            artist_id: 1,
            year: Some(2024),
            genre: None,
            artwork_path: Some("/nonexistent-oxhidifi-cover.jpg".into()),
            track_count: 12,
            total_duration: 3600.0,
            format_summary: "FLAC".into(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96000),
        };
        let (card, overlay) = build_album_card(
            state,
            &album,
            "Test Artist",
            &FormatInfo::default(),
            from_size,
        );
        resize_overlay_cover(&overlay, to_size);
        resize_album_card(&overlay, to_size);
        resolve_cover_widget(&state.cover_art_cache, &overlay, 7, to_size);
        let shows_art = overlay
            .child()
            .is_some_and(|child| child.downcast_ref::<Picture>().is_some());
        let (_, natural, _, _) = card.measure(Horizontal, -1);
        (natural, shows_art)
    }

    #[test]
    fn zoom_out_first_visit_keeps_card_width() -> Result<()> {
        let state = Arc::new(covers_state(Vec::new())?);
        let (natural, shows_art) = zoomed_card_natural_width(&state, &[240], 240, 150);
        ensure!(
            natural == 150,
            "a larger interim texture must not inflate the card, got {natural} px"
        );
        ensure!(
            !shows_art,
            "with only a larger texture cached the card must show a placeholder"
        );
        Ok(())
    }

    #[test]
    fn zoom_out_uses_smaller_interim_without_inflation() -> Result<()> {
        let state = Arc::new(covers_state(Vec::new())?);
        let (natural, shows_art) = zoomed_card_natural_width(&state, &[120, 240], 240, 150);
        ensure!(
            natural == 150,
            "a smaller interim texture must keep the card width, got {natural} px"
        );
        ensure!(
            shows_art,
            "a smaller cached texture must be shown as interim art"
        );
        Ok(())
    }
}

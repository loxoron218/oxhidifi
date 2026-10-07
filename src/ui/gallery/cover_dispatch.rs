//! Background cover decoding, cache insertion, and texture application.
//!
//! Dispatches missing cover sizes to the decoder pool and applies results to
//! card overlays, skipping sizes that became stale while decoding.

use std::{collections::HashMap, hash::BuildHasher, path::Path, sync::Arc};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gdk::MemoryTexture,
        glib::spawn_future_local,
        gtk::{ContentFit::Cover, Overlay, Picture, accessible::Property::Label},
        prelude::{AccessibleExtManual, Cast, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    ui::{
        gallery::card::build_placeholder,
        image_decode::{DecodedCover, raw_to_texture},
        texture_pool::{CoverArtCache, dispatch::ArtworkDecodeRequest},
    },
};

/// Apply a decoded texture to an overlay's child, replacing a non-`Picture`
/// child with a new `Picture` when needed.
pub fn apply_texture(overlay: &Overlay, texture: &MemoryTexture, size: i32) {
    let updated = overlay.child().and_then(|c| {
        c.downcast_ref::<Picture>()
            .map(|p| p.set_paintable(Some(texture)))
    });
    if updated.is_none() {
        let picture = Picture::builder()
            .paintable(texture)
            .content_fit(Cover)
            .width_request(size)
            .height_request(size)
            .css_classes(["album-cover"])
            .build();
        picture.update_property(&[Label("Album cover art")]);
        overlay.set_child(Some(&picture));
    }
}

/// Send decoded album cover through the channel, logging on failure.
fn try_send_album_cover(
    tx: &Sender<(usize, i64, DecodedCover)>,
    index: usize,
    album_id: i64,
    decoded: Option<DecodedCover>,
) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send((index, album_id, decoded)) {
        error!(error = %e, "Failed to send decoded album cover to main thread");
    }
}

/// Check the shared [`CoverArtCache`] and dispatch decode requests for missing
/// covers, applying results via a long-lived local future so the channel
/// receiver outlives the background decoder.
pub fn load_cover_art_async(
    state: &Arc<AppState>,
    cover_art_data: &[(i64, usize, String)],
    overlays: &[Overlay],
    cache: &Arc<CoverArtCache>,
    size: i32,
) {
    if cover_art_data.is_empty() {
        return;
    }

    let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
    let mut uncached: Vec<(i64, usize, String)> = Vec::new();

    for (album_id, index, path) in cover_art_data {
        let Some(overlay) = overlays.get(*index) else {
            continue;
        };
        if let Some(texture) = cache.get(*album_id, size) {
            apply_texture(overlay, &texture, size);
            continue;
        }
        uncached.push((*album_id, *index, path.clone()));
    }

    if uncached.is_empty() {
        return;
    }

    for (album_id, index, path) in uncached {
        let tx = tx.clone();
        cache.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size,
            on_complete: Box::new(move |_, decoded| {
                try_send_album_cover(&tx, index, album_id, decoded);
            }),
        });
    }
    drop(tx);

    let overlays: Vec<Overlay> = overlays.to_vec();
    let cache_clone = Arc::clone(cache);

    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            while let Ok((index, album_id, decoded)) = rx.recv().await {
                apply_decoded_cover_if_current(
                    &cache_clone,
                    &overlays,
                    index,
                    album_id,
                    &decoded,
                    size,
                );
            }
        }));
}

/// Insert a decoded cover into the cache and apply it when still current.
///
/// The texture is always inserted, even when the overlay already resized to
/// a newer zoom size: a superseded `A→B→A` wave still warms the cache so the
/// return to `A` hits instead of re-decoding. Application to the widget is
/// skipped when stale (the card's `width_request` moved on and its own wave
/// owns it).
fn apply_decoded_cover_if_current(
    cache: &CoverArtCache,
    overlays: &[Overlay],
    index: usize,
    album_id: i64,
    decoded: &DecodedCover,
    size: i32,
) {
    let texture = raw_to_texture(decoded);
    cache.insert(album_id, size, texture.clone());
    let Some(overlay) = overlays.get(index) else {
        return;
    };
    if overlay.width_request() != size {
        return;
    }
    apply_decoded_cover(overlays, index, &texture, size);
}

/// Apply a decoded cover to its card, guarding against a size that became
/// stale between the pre-texture guard and the widget apply.
fn apply_decoded_cover(overlays: &[Overlay], index: usize, texture: &MemoryTexture, size: i32) {
    if let Some(overlay) = overlays.get(index)
        && overlay.width_request() == size
    {
        apply_texture(overlay, texture, size);
    }
}

/// Resolve a single card's cover from the grid's cover snapshot, if it has
/// one. Applied during a batched zoom resize after the card's geometry pass.
pub fn resolve_cover_at(
    idx_to_album: &HashMap<usize, i64, impl BuildHasher>,
    cover_cache: &CoverArtCache,
    idx: usize,
    overlay: &Overlay,
    size: i32,
) {
    if let Some(&album_id) = idx_to_album.get(&idx) {
        resolve_cover_widget(cover_cache, overlay, album_id, size);
    }
}

/// Resolve a single card's cover widget to `size` without ever inflating layout.
///
/// Applies the exact cached texture when present, otherwise the largest
/// cached size at or below `size` (upscales inside the fixed requests
/// without changing layout). A larger cached texture is never applied as an
/// interim: its paintable intrinsic size would inflate the card's natural
/// width past the cover, showing an oversized card until the exact size
/// decodes. When no fitting texture exists, an oversized `Picture` is swapped
/// back to a fixed-size placeholder while an existing placeholder (already
/// resized by the geometry pass) is left untouched.
/// Does not dispatch new decode requests; the debounced resize dispatches
/// the decode wave after the traversal completes.
pub fn resolve_cover_widget(cache: &CoverArtCache, overlay: &Overlay, album_id: i64, size: i32) {
    if let Some(texture) = cache.get(album_id, size) {
        apply_texture(overlay, &texture, size);
        return;
    }
    if let Some(texture) = cache.get_fitting(album_id, size) {
        apply_texture(overlay, &texture, size);
        return;
    }
    if overlay
        .child()
        .is_some_and(|child| child.downcast_ref::<Picture>().is_some())
    {
        overlay.set_child(Some(&build_placeholder(size)));
    }
}

/// Report whether every album cover is decoded at `size`.
///
/// Geometry alone cannot tell a completed zoom from a half-done one: the
/// preview flips all overlays to `size` without dispatching decodes, so a
/// geometry-only check would skip the debounced resize that owns the decode
/// wave. Cards left on a larger interim texture keep a paintable whose
/// intrinsic size exceeds `size`, inflating the card's natural width past the
/// cover (grey margin on the right). Both geometry and covers must be ready
/// to skip the resize.
///
/// Entries whose files no longer exist are ignored: their decodes are
/// dropped silently and would otherwise block the fast path forever.
///
/// # Arguments
///
/// * `state` - Application state (cover snapshot + decode cache).
/// * `size` - Cover size the debounced resize would dispatch.
///
/// # Returns
///
/// `true` when every album with artwork either has `size` cached or its file
/// is gone.
#[must_use]
pub fn album_covers_ready(state: &AppState, size: i32) -> bool {
    let covers = Arc::clone(&*state.album_grid_covers.lock());
    let cache = Arc::clone(&state.cover_art_cache);
    covers.iter().all(|(album_id, _, path)| {
        cache.get(*album_id, size).is_some() || !Path::new(path).exists()
    })
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        async_channel::unbounded,
        libadwaita::gtk::{self, test},
    };

    use crate::ui::{
        gallery::cover_dispatch::try_send_album_cover, image_decode::DecodedCover,
        texture_pool::dispatch::tests::mock_decoded_cover,
    };

    #[test]
    fn try_send_album_cover_none_is_noop() -> Result<()> {
        let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
        try_send_album_cover(&tx, 0, 1, None);
        ensure!(rx.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn try_send_album_cover_forwards_decoded() -> Result<()> {
        let (tx, rx) = unbounded::<(usize, i64, DecodedCover)>();
        try_send_album_cover(&tx, 3, 9, Some(mock_decoded_cover()));
        ensure!(matches!(rx.try_recv(), Ok((3, 9, _))));
        Ok(())
    }
}

//! Background cover decoding, cache insertion, and texture application.
//!
//! Dispatches missing cover sizes to the decoder pool and applies results to
//! card overlays, skipping sizes that became stale while decoding.

use std::sync::Arc;

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

/// Insert a decoded cover into the cache and apply it, skipping sizes that
/// became stale before the texture was allocated.
///
/// Staleness is read from the card itself: a zoom or narrow-window fit resize
/// since dispatch changed the overlay's minimum width, so a size mismatch
/// means a newer size (with its own decode wave) owns the card.
fn apply_decoded_cover_if_current(
    cache: &CoverArtCache,
    overlays: &[Overlay],
    index: usize,
    album_id: i64,
    decoded: &DecodedCover,
    size: i32,
) {
    let Some(overlay) = overlays.get(index) else {
        return;
    };
    if overlay.width_request() != size {
        return;
    }
    let texture = raw_to_texture(decoded);
    cache.insert(album_id, size, texture.clone());
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

//! Decode request dispatch and the background cover-decoder worker pool.

use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    sync::{Arc, Weak},
};

use {
    async_channel::{Receiver, Sender, unbounded},
    tracing::error,
};

use crate::{
    threading::ThreadManager,
    ui::{
        image_decode::{DecodedCover, decode_cover_raw},
        texture_pool::CoverArtCache,
        zoom::GRID_ZOOM_MAX,
    },
};

/// Maximum number of decoded sizes retained per album.
///
/// Covers every grid zoom level (0–[`GRID_ZOOM_MAX`]) so a full zoom sweep
/// never re-decodes a size that was already decoded this session — repeated
/// zoom toggling stops re-dispatching after the first sweep. Column-view
/// list sizes are smaller and evicted by grid covers, which is acceptable:
/// grid/list switches are rare, and a re-decode on switch is bounded.
pub const MAX_SIZES_PER_ALBUM: u8 = GRID_ZOOM_MAX + 1;

/// Number of worker threads decoding cover art.
///
/// Decode requests only originate from debounced user actions (zoom or
/// sort rebuilds), so the queue stays bounded; a small pool keeps each
/// full-grid re-decode wave well below the latency of a single thread.
const COVER_DECODER_THREADS: usize = 3;

/// Request for the centralized cover decoder worker.
pub struct ArtworkDecodeRequest {
    /// Album database ID.
    pub album_id: i64,
    /// File path to the cover image.
    pub path: String,
    /// Target decode size (width and height).
    pub size: i32,
    /// Callback invoked on the worker thread with the decode result.
    pub on_complete: Box<dyn FnOnce(i64, Option<DecodedCover>) + Send + 'static>,
}

impl Debug for ArtworkDecodeRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ArtworkDecodeRequest")
            .field("album_id", &self.album_id)
            .field("path", &self.path)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl CoverArtCache {
    /// Create a new `CoverArtCache` wrapped in [`Arc`].
    ///
    /// Spawns a pool of [`COVER_DECODER_THREADS`] background threads
    /// (`"cover-decoder-{i}"`) via the [`ThreadManager`] that process
    /// decode requests concurrently.
    pub fn new_shared(thread_manager: &ThreadManager) -> Arc<Self> {
        let (request_tx, request_rx) = unbounded::<ArtworkDecodeRequest>();

        for i in 0..COVER_DECODER_THREADS {
            spawn_cover_decoder(thread_manager, i, request_rx.clone());
        }

        Arc::new(Self::with_sender(Some(request_tx)))
    }

    /// Send a cover decode request to the background worker.
    ///
    /// Coalesces duplicate `(album_id, size)` requests: while a decode for
    /// that key is queued or in flight, later requests are dropped. The key
    /// is released when the decode completes (or if the send fails), so
    /// rapid zoom toggling never queues the same cover twice yet a later
    /// zoom to the same size can still re-decode after eviction.
    pub fn request_decode(self: &Arc<Self>, request: ArtworkDecodeRequest) {
        let key = (request.album_id, request.size);
        if !self.in_flight.lock().insert(key) {
            return;
        }
        let weak = Arc::downgrade(self);
        let on_complete = request.on_complete;
        let size = request.size;
        let request = ArtworkDecodeRequest {
            album_id: request.album_id,
            path: request.path,
            size,
            on_complete: Box::new(move |aid, decoded| {
                release_in_flight(&weak, aid, size);
                on_complete(aid, decoded);
            }),
        };
        if let Some(tx) = self.request_tx.lock().as_ref()
            && let Err(e) = tx.try_send(request)
        {
            _ = self.in_flight.lock().remove(&key);
            error!(error = %e, "Failed to send cover decode request");
        }
    }

    /// Request decoding and send the result through a channel.
    ///
    /// The requested `size` is sent alongside the decoded cover so callers
    /// can key the cache by the requested size rather than the actual
    /// decoded width, which may differ for non-square artwork.
    pub fn request_decode_to_channel(
        self: &Arc<Self>,
        album_id: i64,
        path: String,
        size: i32,
        tx: Sender<(i64, i32, DecodedCover)>,
        error_context: &'static str,
    ) {
        self.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size,
            on_complete: Box::new(move |aid, decoded| {
                send_channel_cover(&tx, aid, size, decoded, error_context);
            }),
        });
    }

    /// Drop the outgoing request sender, closing the channel.
    ///
    /// This causes the background cover-decoder threads to exit their
    /// `recv_blocking` loops, allowing `ThreadManager::shutdown` to
    /// join them without hanging.
    pub fn shutdown(&self) {
        drop(self.request_tx.lock().take());
    }
}

/// Remove an `(album_id, size)` key from the in-flight dedup set once its
/// decode completes. No-op when the cache was dropped (shutdown).
fn release_in_flight(cache: &Weak<CoverArtCache>, album_id: i64, size: i32) {
    if let Some(cache) = cache.upgrade() {
        _ = cache.in_flight.lock().remove(&(album_id, size));
    }
}

/// Spawn one named cover decoder worker thread.
fn spawn_cover_decoder(
    thread_manager: &ThreadManager,
    index: usize,
    rx: Receiver<ArtworkDecodeRequest>,
) {
    thread_manager.spawn_named(&format!("cover-decoder-{index}"), move || {
        run_cover_decoder(&rx);
    });
}

/// Run a background cover decoder loop.
///
/// Consumes decode requests from the shared receiver until the channel
/// is closed (during shutdown).  Each worker owns its own receiver
/// clone; requests are distributed across the pool by the channel.
fn run_cover_decoder(rx: &Receiver<ArtworkDecodeRequest>) {
    while let Ok(req) = rx.recv_blocking() {
        let decoded = decode_cover_raw(&req.path, req.size);
        (req.on_complete)(req.album_id, decoded);
    }
}

/// Try to send decoded cover through a channel, logging on failure.
pub fn send_channel_cover(
    tx: &Sender<(i64, i32, DecodedCover)>,
    aid: i64,
    size: i32,
    decoded: Option<DecodedCover>,
    context: &str,
) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send((aid, size, decoded)) {
        error!(error = %e, "Failed to send decoded cover to {context}");
    }
}

/// Unit tests for cover decode dispatch, channel forwarding, and test helpers.
#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Error, Result, ensure},
        async_channel::{Sender, unbounded},
        libadwaita::gdk::MemoryFormat::R8g8b8a8,
    };

    use crate::ui::{
        image_decode::DecodedCover,
        texture_pool::{
            CoverArtCache,
            dispatch::{ArtworkDecodeRequest, send_channel_cover},
        },
    };

    /// Build a minimal decoded cover for channel-send tests.
    #[must_use]
    pub fn mock_decoded_cover() -> DecodedCover {
        DecodedCover {
            width: 2,
            height: 2,
            rowstride: 8,
            format: R8g8b8a8,
            data: vec![0; 16],
        }
    }

    /// Verify that a cover-send helper ignores a `None` decode result.
    ///
    /// # Arguments
    ///
    /// * `send` - The `try_send`-style helper under test
    ///
    /// # Errors
    ///
    /// Returns an error if the channel received a value after sending `None`.
    pub fn cover_send_none_is_noop(
        send: fn(&Sender<DecodedCover>, Option<DecodedCover>),
    ) -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        send(&tx, None);
        ensure!(rx.try_recv().is_err());
        Ok(())
    }

    /// Verify that a cover-send helper forwards a decoded cover through its channel.
    ///
    /// # Arguments
    ///
    /// * `send` - The `try_send`-style helper under test
    ///
    /// # Errors
    ///
    /// Returns an error if the channel received no value after sending a cover.
    pub fn cover_send_forwards_decoded(
        send: fn(&Sender<DecodedCover>, Option<DecodedCover>),
    ) -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        send(&tx, Some(mock_decoded_cover()));
        ensure!(rx.try_recv().is_ok());
        Ok(())
    }

    #[test]
    fn send_channel_cover_none_is_noop() -> Result<()> {
        let (tx, rx) = unbounded::<(i64, i32, DecodedCover)>();
        send_channel_cover(&tx, 1, 64, None, "test");
        ensure!(rx.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn send_channel_cover_forwards_decoded() -> Result<()> {
        let (tx, rx) = unbounded::<(i64, i32, DecodedCover)>();
        send_channel_cover(&tx, 7, 64, Some(mock_decoded_cover()), "test");
        ensure!(matches!(rx.try_recv(), Ok((7, 64, _))));
        Ok(())
    }

    #[test]
    fn request_decode_dedups_in_flight_requests() -> Result<()> {
        let (tx, rx) = unbounded::<ArtworkDecodeRequest>();
        let cache = Arc::new(CoverArtCache::with_sender(Some(tx)));

        let request = |path: &str| ArtworkDecodeRequest {
            album_id: 1,
            path: path.to_string(),
            size: 180,
            on_complete: Box::new(|_, _| {}),
        };

        cache.request_decode(request("a.jpg"));
        cache.request_decode(request("a.jpg"));
        ensure!(
            rx.len() == 1,
            "a duplicate (album, size) request must be dropped while in flight"
        );

        let queued = rx.try_recv().map_err(Error::msg)?;
        (queued.on_complete)(1, None);
        ensure!(
            cache.in_flight.lock().is_empty(),
            "completing a decode must release its in-flight key"
        );

        cache.request_decode(request("b.jpg"));
        ensure!(
            rx.len() == 1,
            "a new request after completion must be accepted"
        );
        Ok(())
    }
}

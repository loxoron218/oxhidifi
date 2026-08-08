//! Libadwaita UI components: window, header, library views, detail pages, player panel.

pub mod detail;
pub mod header;
pub mod library;
pub mod player;
pub mod settings;
pub mod settings_audio;
pub mod settings_library;
pub mod settings_view;
pub mod signal;
pub mod sort_list;
pub mod status;
pub mod window;
pub mod window_navigation;
pub mod window_panes;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        gdk::{
            MemoryFormat::{self, R8g8b8, R8g8b8a8},
            MemoryTexture,
        },
        glib::Bytes,
        gtk::{Align::Center, Button, accessible::Property::Label, gdk_pixbuf::Pixbuf},
        prelude::AccessibleExtManual,
    },
    parking_lot::Mutex,
    tracing::error,
};

use crate::{threading::ThreadManager, zoom::GRID_ZOOM_MAX};

/// Maximum number of decoded sizes retained per album.
///
/// Covers every grid zoom level (0–[`GRID_ZOOM_MAX`]) so a full zoom sweep
/// never re-decodes a size that was already decoded this session — repeated
/// zoom toggling stops re-dispatching after the first sweep. Column-view
/// list sizes are smaller and evicted by grid covers, which is acceptable:
/// grid/list switches are rare, and a re-decode on switch is bounded.
const MAX_SIZES_PER_ALBUM: usize = GRID_ZOOM_MAX as usize + 1;

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

/// Thread-safe cache for decoded cover art textures.
///
/// Keyed by `(album_id, size)` so that covers decoded at different
/// zoom levels or view sizes are independently cached.  A small fixed
/// pool of background worker threads processes decode requests
/// concurrently, preventing a full-grid re-decode wave from blocking
/// the UI for seconds on large libraries.
///
/// Both the grid view and column view share the same cache instance,
/// and texture lookups without a specific size (e.g. the player panel)
/// return any cached size for the album.
pub struct CoverArtCache {
    /// Guarded cache state: decoded textures and per-album size indices.
    inner: Mutex<CoverCacheInner>,
    /// Map of track ID to album ID, so cover lookups by `track_id` can
    /// resolve to the correct album-level cache entry.
    track_to_album: Mutex<HashMap<i64, i64>>,
    /// `(album_id, size)` keys with a decode request queued or in flight,
    /// so rapid zoom toggling never dispatches the same album×size twice.
    in_flight: Mutex<HashSet<(i64, i32)>>,
    /// Channel sender for dispatching decode requests to the worker.
    /// Wrapped in `Mutex<Option<...>>` so the channel can be closed
    /// during shutdown, allowing the worker thread to exit.
    request_tx: Mutex<Option<Sender<ArtworkDecodeRequest>>>,
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

    /// Build an empty cache around an optional request sender.
    ///
    /// `new_shared` spawns the decoder pool and passes its sender; tests
    /// pass `None` to construct a cache without any worker threads.
    fn with_sender(request_tx: Option<Sender<ArtworkDecodeRequest>>) -> Self {
        Self {
            inner: Mutex::new(CoverCacheInner {
                textures: HashMap::new(),
                sizes: HashMap::new(),
            }),
            track_to_album: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            request_tx: Mutex::new(request_tx),
        }
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
            self.in_flight.lock().remove(&key);
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

    /// Return the cached texture for a given album ID at a specific size.
    pub fn get(&self, album_id: i64, size: i32) -> Option<Arc<MemoryTexture>> {
        self.inner.lock().textures.get(&(album_id, size)).cloned()
    }

    /// Return a cached texture for an album at the most recently stored size.
    pub fn get_any(&self, album_id: i64) -> Option<Arc<MemoryTexture>> {
        let inner = self.inner.lock();
        let size = *inner.sizes.get(&album_id)?.last()?;
        inner.textures.get(&(album_id, size)).cloned()
    }

    /// Check whether any texture is cached for the given album.
    pub fn has_any(&self, album_id: i64) -> bool {
        self.inner.lock().sizes.contains_key(&album_id)
    }

    /// Insert a decoded texture into the cache by album ID and size.
    ///
    /// The size is recorded as the most recent for the album; if the album
    /// already has [`MAX_SIZES_PER_ALBUM`] sizes, the oldest is evicted.
    pub fn insert(&self, album_id: i64, size: i32, texture: MemoryTexture) {
        let mut inner = self.inner.lock();
        inner.textures.insert((album_id, size), Arc::new(texture));
        let sizes = inner.sizes.entry(album_id).or_default();
        if let Some(pos) = sizes.iter().position(|&s| s == size) {
            sizes.remove(pos);
        }
        sizes.push(size);
        let evicted: Vec<i32> = sizes
            .drain(..sizes.len().saturating_sub(MAX_SIZES_PER_ALBUM))
            .collect();
        for old in evicted {
            inner.textures.remove(&(album_id, old));
        }
    }

    /// Record the album that a track belongs to, enabling cache lookups
    /// by track ID to resolve to the album-level cache entry.
    pub fn record_track_album(&self, track_id: i64, album_id: i64) {
        self.track_to_album.lock().insert(track_id, album_id);
    }

    /// Look up a cached cover texture by track ID.
    ///
    /// Resolves `track_id → album_id` using the recorded track-to-album
    /// mapping, then returns any cached texture for that album.
    /// Returns `None` if either the mapping or any texture is missing.
    pub fn get_by_track(&self, track_id: i64) -> Option<Arc<MemoryTexture>> {
        let album_id = *self.track_to_album.lock().get(&track_id)?;
        self.get_any(album_id)
    }

    /// Return the cached album ID for a track, if previously recorded.
    pub fn get_album_for_track(&self, track_id: i64) -> Option<i64> {
        self.track_to_album.lock().get(&track_id).copied()
    }

    /// Drop the outgoing request sender, closing the channel.
    ///
    /// This causes the background cover-decoder threads to exit their
    /// `recv_blocking` loops, allowing `ThreadManager::shutdown` to
    /// join them without hanging.
    pub fn shutdown(&self) {
        self.request_tx.lock().take();
    }
}

/// Inner state of the cover art cache, guarded by a single mutex.
///
/// `textures` and `sizes` live under one lock so that eviction can remove
/// from both without cross-lock ordering or a consistency window.
struct CoverCacheInner {
    /// Map of `(album_id, size)` to decoded texture.
    textures: HashMap<(i64, i32), Arc<MemoryTexture>>,
    /// Map of album ID to the sizes currently cached, oldest first.
    sizes: HashMap<i64, Vec<i32>>,
}

/// Decoded cover art as raw pixel data (Send-safe).
pub struct DecodedCover {
    /// Image width in pixels.
    pub width: i32,
    /// Image height in pixels.
    pub height: i32,
    /// Row stride in bytes.
    pub rowstride: usize,
    /// Pixel format.
    pub format: MemoryFormat,
    /// Raw pixel data.
    pub data: Vec<u8>,
}

/// Remove an `(album_id, size)` key from the in-flight dedup set once its
/// decode completes. No-op when the cache was dropped (shutdown).
fn release_in_flight(cache: &Weak<CoverArtCache>, album_id: i64, size: i32) {
    if let Some(cache) = cache.upgrade() {
        cache.in_flight.lock().remove(&(album_id, size));
    }
}

/// Decode an image file at a given size into raw pixel data.
///
/// Returns `None` if the file could not be loaded or decoded.
/// The raw data can be sent across threads and converted to a
/// `MemoryTexture` on the main thread via [`raw_to_texture`].
pub fn decode_cover_raw(path: &str, size: i32) -> Option<DecodedCover> {
    let pixbuf = match Pixbuf::from_file_at_scale(path, size, size, true) {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Failed to decode cover art at {path}");
            return None;
        }
    };
    let format = if pixbuf.has_alpha() { R8g8b8a8 } else { R8g8b8 };
    let bytes = pixbuf.read_pixel_bytes();
    Some(DecodedCover {
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: pixbuf.rowstride().cast_unsigned() as usize,
        format,
        data: bytes.to_vec(),
    })
}

/// Convert raw decoded pixel data into a `MemoryTexture` for painting.
///
/// Must be called on the main thread (creates a `GdkMemoryTexture`).
#[must_use]
pub fn raw_to_texture(decoded: &DecodedCover) -> MemoryTexture {
    let bytes = Bytes::from(&decoded.data[..]);
    MemoryTexture::new(
        decoded.width,
        decoded.height,
        decoded.format,
        &bytes,
        decoded.rowstride,
    )
}

/// Decode an image file at a given size into a `MemoryTexture`.
///
/// Returns `None` if the file could not be loaded or decoded.
pub fn decode_cover_at_size(path: &str, size: i32) -> Option<MemoryTexture> {
    decode_cover_raw(path, size).as_ref().map(raw_to_texture)
}

/// Build a circular OSD play button for album overlays.
#[must_use]
pub fn build_album_play_button() -> Button {
    let btn = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .css_classes(["circular", "osd"])
        .halign(Center)
        .valign(Center)
        .tooltip_text("Play or pause album")
        .can_focus(true)
        .build();
    btn.update_property(&[Label("Play or pause album")]);
    btn
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
fn send_channel_cover(
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Error, Result, ensure},
        async_channel::{Sender, unbounded},
        libadwaita::{
            gdk::MemoryFormat::R8g8b8a8,
            gtk::{self, test},
            prelude::{ButtonExt, WidgetExt},
        },
    };

    use crate::ui::{
        ArtworkDecodeRequest, CoverArtCache, DecodedCover, build_album_play_button, raw_to_texture,
        send_channel_cover,
    };

    fn make_cache() -> CoverArtCache {
        CoverArtCache::with_sender(None)
    }

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
    fn get_by_track_unmapped_returns_none() {
        let cache = make_cache();
        assert!(cache.get_by_track(1).is_none());
        assert!(cache.get_album_for_track(1).is_none());
    }

    #[test]
    fn record_track_album_round_trip() {
        let cache = make_cache();
        assert!(cache.get_album_for_track(10).is_none());
        cache.record_track_album(10, 100);
        assert_eq!(cache.get_album_for_track(10), Some(100));
    }

    #[test]
    fn build_album_play_button_sets_icon_and_tooltip() -> Result<()> {
        let button = build_album_play_button();
        ensure!(button.icon_name().as_deref() == Some("media-playback-start-symbolic"));
        ensure!(button.tooltip_text().as_deref() == Some("Play or pause album"));
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
    fn insert_get_round_trips_by_size() -> Result<()> {
        let cache = make_cache();
        ensure!(cache.get(1, 180).is_none(), "missing size must return None");
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 240, raw_to_texture(&mock_decoded_cover()));
        ensure!(cache.get(1, 180).is_some(), "inserted size must be cached");
        ensure!(cache.get(1, 240).is_some(), "each size must be cached");
        ensure!(
            cache.get(1, 120).is_none(),
            "a size that was never inserted must return None"
        );
        Ok(())
    }

    #[test]
    fn get_any_returns_most_recent_size() -> Result<()> {
        let cache = make_cache();
        ensure!(cache.get_any(1).is_none(), "empty cache must return None");
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 240, raw_to_texture(&mock_decoded_cover()));
        let texture = cache.get_any(1);
        ensure!(texture.is_some(), "get_any must find a cached size");
        ensure!(
            texture == cache.get(1, 240),
            "get_any must return the most recently inserted size"
        );
        Ok(())
    }

    #[test]
    fn has_any_reflects_cached_sizes() -> Result<()> {
        let cache = make_cache();
        ensure!(!cache.has_any(1), "empty cache must report no sizes");
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        ensure!(cache.has_any(1), "cached album must report a size");
        ensure!(!cache.has_any(2), "other albums must stay uncached");
        Ok(())
    }

    #[test]
    fn eviction_keeps_only_newest_sizes() -> Result<()> {
        let cache = make_cache();
        for size in [120, 150, 180, 210, 240, 32] {
            cache.insert(1, size, raw_to_texture(&mock_decoded_cover()));
        }
        ensure!(
            cache.get(1, 120).is_none(),
            "the oldest size must be evicted past MAX_SIZES_PER_ALBUM"
        );
        ensure!(
            cache.get(1, 150).is_some(),
            "the second-oldest size must survive eviction"
        );
        ensure!(
            cache.get(1, 32).is_some(),
            "the newest size must survive eviction"
        );
        Ok(())
    }

    #[test]
    fn reinserting_same_size_does_not_count_twice() -> Result<()> {
        let cache = make_cache();
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 120, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 210, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 240, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 32, raw_to_texture(&mock_decoded_cover()));
        ensure!(
            cache.get(1, 120).is_some(),
            "a re-inserted size must not count twice and evict an earlier size"
        );
        ensure!(
            cache.get(1, 180).is_some(),
            "the re-inserted size must still be cached"
        );
        ensure!(
            cache.get_any(1) == cache.get(1, 32),
            "the most recent insertion must win get_any"
        );
        Ok(())
    }

    #[test]
    fn get_by_track_resolves_any_cached_size() -> Result<()> {
        let cache = make_cache();
        ensure!(
            cache.get_by_track(10).is_none(),
            "unmapped track must return None"
        );
        cache.record_track_album(10, 100);
        ensure!(
            cache.get_by_track(10).is_none(),
            "a mapped track without a cached cover must return None"
        );
        cache.insert(100, 240, raw_to_texture(&mock_decoded_cover()));
        ensure!(
            cache.get_by_track(10).is_some(),
            "a mapped track must resolve to a cached cover"
        );
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

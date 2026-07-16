//! Libadwaita UI components: window, header, library views, detail pages, player panel.

pub mod detail;
pub mod header;
pub mod library;
pub mod player;
pub mod settings;
pub mod status;
pub mod window;

use std::{collections::HashMap, sync::Arc};

use crate::threading::ThreadManager;

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        gdk::{
            MemoryFormat::{self, R8g8b8, R8g8b8a8},
            MemoryTexture,
        },
        glib::Bytes,
        gtk::gdk_pixbuf::Pixbuf,
    },
    parking_lot::Mutex,
    tracing::error,
};

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
/// Keyed by album database ID.  A single background worker thread
/// processes all decode requests sequentially, preventing decoder
/// pool saturation and duplicate work across views.
///
/// Both the grid view and column view share the same cache instance
/// so that each album cover is only decoded once per session, even
/// when switching between view modes.
pub struct CoverArtCache {
    /// Map of album ID to decoded texture.
    textures: Mutex<HashMap<i64, Arc<MemoryTexture>>>,
    /// Map of track ID to album ID, so cover lookups by `track_id` can
    /// resolve to the correct album-level cache entry.
    track_to_album: Mutex<HashMap<i64, i64>>,
    /// Channel sender for dispatching decode requests to the worker.
    /// Wrapped in `Mutex<Option<...>>` so the channel can be closed
    /// during shutdown, allowing the worker thread to exit.
    request_tx: Mutex<Option<Sender<ArtworkDecodeRequest>>>,
}

impl CoverArtCache {
    /// Create a new `CoverArtCache` wrapped in [`Arc`].
    ///
    /// Spawns a single background thread (`"cover-decoder"`) via the
    /// [`ThreadManager`] that processes decode requests sequentially.
    pub fn new_shared(thread_manager: &ThreadManager) -> Arc<Self> {
        let (request_tx, request_rx) = unbounded::<ArtworkDecodeRequest>();

        thread_manager.spawn_named("cover-decoder", move || {
            run_cover_decoder(&request_rx);
        });

        Arc::new(Self {
            textures: Mutex::new(HashMap::new()),
            track_to_album: Mutex::new(HashMap::new()),
            request_tx: Mutex::new(Some(request_tx)),
        })
    }

    /// Send a cover decode request to the background worker.
    pub fn request_decode(&self, request: ArtworkDecodeRequest) {
        if let Some(tx) = self.request_tx.lock().as_ref()
            && let Err(e) = tx.try_send(request)
        {
            error!(error = %e, "Failed to send cover decode request");
        }
    }

    /// Request decoding and send the result through a channel.
    pub fn request_decode_to_channel(
        &self,
        album_id: i64,
        path: String,
        size: i32,
        tx: Sender<(i64, DecodedCover)>,
        error_context: &'static str,
    ) {
        self.request_decode(ArtworkDecodeRequest {
            album_id,
            path,
            size,
            on_complete: Box::new(move |aid, decoded| {
                send_channel_cover(&tx, aid, decoded, error_context);
            }),
        });
    }

    /// Return the cached texture for a given album ID, if available.
    pub fn get(&self, album_id: i64) -> Option<Arc<MemoryTexture>> {
        self.textures.lock().get(&album_id).cloned()
    }

    /// Insert a decoded texture into the cache by album ID.
    pub fn insert(&self, album_id: i64, texture: MemoryTexture) {
        self.textures.lock().insert(album_id, Arc::new(texture));
    }

    /// Record the album that a track belongs to, enabling cache lookups
    /// by track ID to resolve to the album-level cache entry.
    pub fn record_track_album(&self, track_id: i64, album_id: i64) {
        self.track_to_album.lock().insert(track_id, album_id);
    }

    /// Look up a cached cover texture by track ID.
    ///
    /// Resolves `track_id → album_id → texture` using the recorded
    /// track-to-album mapping.  Returns `None` if either the mapping
    /// or the album-level texture is missing.
    pub fn get_by_track(&self, track_id: i64) -> Option<Arc<MemoryTexture>> {
        let album_id = *self.track_to_album.lock().get(&track_id)?;
        self.textures.lock().get(&album_id).cloned()
    }

    /// Return the cached album ID for a track, if previously recorded.
    pub fn get_album_for_track(&self, track_id: i64) -> Option<i64> {
        self.track_to_album.lock().get(&track_id).copied()
    }

    /// Drop the outgoing request sender, closing the channel.
    ///
    /// This causes the background cover-decoder thread to exit its
    /// `recv_blocking` loop, allowing `ThreadManager::shutdown` to
    /// join it without hanging.
    pub fn shutdown(&self) {
        self.request_tx.lock().take();
    }
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

/// Run the background cover decoder loop.
fn run_cover_decoder(rx: &Receiver<ArtworkDecodeRequest>) {
    while let Ok(req) = rx.recv_blocking() {
        let decoded = decode_cover_raw(&req.path, req.size);
        (req.on_complete)(req.album_id, decoded);
    }
}

/// Try to send decoded cover through a channel, logging on failure.
fn send_channel_cover(
    tx: &Sender<(i64, DecodedCover)>,
    aid: i64,
    decoded: Option<DecodedCover>,
    context: &str,
) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send((aid, decoded)) {
        error!(error = %e, "Failed to send decoded cover to {context}");
    }
}

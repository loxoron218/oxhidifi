//! Libadwaita UI components: window, header, library views, detail pages, player panel.

pub mod detail;
pub mod header;
pub mod library;
pub mod player;
pub mod settings;
pub mod status;
pub mod window;

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
};

use {
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

/// Thread-safe cache for decoded cover art textures.
///
/// Keyed by album database ID.  Decoding is performed in a single
/// background batch to avoid flooding the Glycin sandboxed decoder
/// pool.  The `batch_in_progress` flag prevents multiple views from
/// starting simultaneous decode batches.
///
/// Both the grid view and column view share the same cache instance
/// so that each album cover is only decoded once per session, even
/// when switching between view modes.
pub struct CoverArtCache {
    /// Map of album ID to decoded texture.
    textures: Mutex<HashMap<i64, Arc<MemoryTexture>>>,
    /// True while a batch decode is in progress.
    batch_in_progress: AtomicBool,
}

impl CoverArtCache {
    /// Create a new `CoverArtCache` wrapped in [`Arc`].
    #[must_use]
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self {
            textures: Mutex::new(HashMap::new()),
            batch_in_progress: AtomicBool::new(false),
        })
    }

    /// Return the cached texture for a given album ID, if available.
    #[must_use]
    pub fn get(&self, album_id: i64) -> Option<Arc<MemoryTexture>> {
        self.textures.lock().get(&album_id).cloned()
    }

    /// Insert a decoded texture into the cache by album ID.
    pub fn insert(&self, album_id: i64, texture: MemoryTexture) {
        self.textures.lock().insert(album_id, Arc::new(texture));
    }

    /// Atomically claim the decode-batch flag.
    ///
    /// Returns `true` if no batch was in progress and this caller is
    /// now responsible for the batch.  Returns `false` if a batch is
    /// already running (another view is decoding).
    pub fn try_start_batch(&self) -> bool {
        self.batch_in_progress
            .compare_exchange(false, true, SeqCst, SeqCst)
            .is_ok()
    }

    /// Clear the batch-in-progress flag after decoding completes.
    pub fn finish_batch(&self) {
        self.batch_in_progress.store(false, SeqCst);
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

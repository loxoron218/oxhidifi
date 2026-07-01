//! Libadwaita UI components: window, header, library views, detail pages, player panel.

pub mod detail;
pub mod header;
pub mod library;
pub mod player;
pub mod settings;
pub mod status;
pub mod window;

use {
    libadwaita::{
        gdk::{
            MemoryFormat::{self, R8g8b8, R8g8b8a8},
            MemoryTexture,
        },
        glib::Bytes,
        gtk::gdk_pixbuf::Pixbuf,
    },
    tracing::error,
};

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

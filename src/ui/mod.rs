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
            MemoryFormat::{R8g8b8, R8g8b8a8},
            MemoryTexture,
        },
        gtk::gdk_pixbuf::Pixbuf,
    },
    tracing::error,
};

/// Decode an image file at a given size into a `MemoryTexture`.
///
/// Returns `None` if the file could not be loaded or decoded.
pub fn decode_cover_at_size(path: &str, size: i32) -> Option<MemoryTexture> {
    let pixbuf = match Pixbuf::from_file_at_scale(path, size, size, true) {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Failed to decode cover art at {path}");
            return None;
        }
    };
    let format = if pixbuf.has_alpha() { R8g8b8a8 } else { R8g8b8 };
    let bytes = pixbuf.read_pixel_bytes();
    Some(MemoryTexture::new(
        pixbuf.width(),
        pixbuf.height(),
        format,
        &bytes,
        pixbuf.rowstride().cast_unsigned() as usize,
    ))
}

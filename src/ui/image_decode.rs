//! Image file decoding into raw pixel data and `MemoryTexture`.

use std::path::Path;

use {
    libadwaita::{
        gdk::{
            MemoryFormat::{self, R8g8b8, R8g8b8a8},
            MemoryTexture,
        },
        glib::Bytes,
        gtk::gdk_pixbuf::{InterpType::Hyper, Pixbuf},
    },
    tracing::warn,
};

use crate::library::artwork::thumbnail::scaled_dimensions;

/// Minimum decode size using the high-quality scaling path.
///
/// Player (560 px) and detail (320 px) covers decode from the full original
/// with `Hyper` interpolation so `HiDPI` panels stay sharp. Smaller grid/list
/// thumbnails keep the fast `from_file_at_scale` path.
const HIGH_QUALITY_MIN_SIZE: i32 = 280;

/// Decoded cover art as raw pixel data (Send-safe).
#[derive(Debug)]
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
/// Large covers (`size >= 280`) decode the full original and downscale with
/// high-quality `Hyper` interpolation; smaller sizes keep the fast loader
/// path to avoid slowing full-grid waves.
///
/// Returns `None` if the file could not be loaded or decoded.
/// A missing file is an expected condition after an artwork cache wipe
/// (see `check_cache_version`) and is skipped silently so a full grid of
/// stale album paths does not flood the log. Callers filter missing files
/// before dispatching (see `CoverArtCache::request_decode`), so reaching
/// this branch means the file vanished mid-request.
/// Genuinely unreadable files are logged at `warn` level.
/// The raw data can be sent across threads and converted to a
/// `MemoryTexture` on the main thread via [`raw_to_texture`].
pub fn decode_cover_raw(path: &str, size: i32) -> Option<DecodedCover> {
    if !Path::new(path).exists() {
        return None;
    }
    let pixbuf = if size >= HIGH_QUALITY_MIN_SIZE {
        decode_high_quality(path, size)?
    } else {
        match Pixbuf::from_file_at_scale(path, size, size, true) {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, path, "Failed to decode cover art");
                return None;
            }
        }
    };
    let format = if pixbuf.has_alpha() { R8g8b8a8 } else { R8g8b8 };
    let bytes = pixbuf.read_pixel_bytes();
    Some(DecodedCover {
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: usize::try_from(pixbuf.rowstride().cast_unsigned()).unwrap_or(0),
        format,
        data: bytes.to_vec(),
    })
}

/// Decode the full original and downscale with high-quality interpolation.
///
/// Preserves aspect ratio, fitting the longer edge to `size`.
///
/// # Arguments
///
/// * `path` - Image file to decode.
/// * `size` - Target bounding size in pixels.
///
/// # Returns
///
/// The downscaled `Pixbuf`, or `None` on load/scale failure.
fn decode_high_quality(path: &str, size: i32) -> Option<Pixbuf> {
    let full = match Pixbuf::from_file(path) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, path, "Failed to decode cover art");
            return None;
        }
    };
    let (target_w, target_h) = scaled_dimensions(full.width(), full.height(), size);
    full.scale_simple(target_w, target_h, Hyper).map_or_else(
        || {
            warn!(
                path,
                size, "Failed to scale cover art with high-quality filter"
            );
            None
        },
        Some,
    )
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

#[cfg(test)]
mod tests {
    use crate::ui::image_decode::decode_cover_at_size;

    #[test]
    fn missing_cover_decodes_to_none() {
        assert!(
            decode_cover_at_size("/nonexistent-oxhidifi-cover", 64).is_none(),
            "missing file must decode to None"
        );
    }
}

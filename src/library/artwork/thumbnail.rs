//! Thumbnail generation for cached artwork.
//!
//! Downscales cached originals into fixed sizes for grid and list views,
//! isolating the `gdk-pixbuf` dependency to this child module.

use std::path::Path;

use {
    libadwaita::gtk::gdk_pixbuf::{InterpType::Bilinear, PixbufLoader, prelude::PixbufLoaderExt},
    tracing::warn,
};

/// Thumbnail sizes generated for grid/column views (px).
///
/// Covers the default grid (180 px) and list (48 px) sizes plus the extremes so
/// every zoom level has a close thumbnail on disk. The full-size original is
/// always cached alongside these.
const THUMBNAIL_SIZES: &[i32] = &[32, 48, 64, 120, 150, 180, 210, 240];

/// Generate downscaled thumbnails for grid/column views.
///
/// Thumbnails are written as `{key}_{size}.{ext}` for each size in
/// [`THUMBNAIL_SIZES`]. Failures are logged via `tracing::warn` and do not
/// propagate — the original artwork remains usable even if thumbnails cannot be
/// created (e.g., corrupt image bytes or missing `gdk-pixbuf` loader).
pub(super) fn generate_thumbnails(cache_dir: &Path, key: &str, data: &[u8], ext: &str) {
    let loader = PixbufLoader::new();
    if let Err(e) = loader.write(data) {
        warn!(error = %e, key, "Failed to load artwork bytes for thumbnail generation");
        return;
    }
    if let Err(e) = loader.close() {
        warn!(error = %e, key, "Failed to close pixbuf loader for thumbnails");
        return;
    }
    let Some(pixbuf) = loader.pixbuf() else {
        warn!(key, "Pixbuf loader produced no image for thumbnails");
        return;
    };
    let orig_w = pixbuf.width();
    let orig_h = pixbuf.height();
    if orig_w <= 0 || orig_h <= 0 {
        warn!(key, orig_w, orig_h, "Invalid original artwork dimensions");
        return;
    }
    for &size in THUMBNAIL_SIZES {
        let (thumb_w, thumb_h) = scaled_dimensions(orig_w, orig_h, size);
        let Some(scaled) = pixbuf.scale_simple(thumb_w, thumb_h, Bilinear) else {
            warn!(key, size, "Failed to scale artwork thumbnail");
            continue;
        };
        let thumb_path = cache_dir.join(format!("{key}_{size}.{ext}"));
        let save_type = match ext {
            "jpg" | "jpeg" => "jpeg",
            "webp" => "webp",
            _ => "png",
        };
        if let Err(e) = scaled.savev(&thumb_path, save_type, &[]) {
            warn!(error = %e, key, size, path = %thumb_path.display(), "Failed to save artwork thumbnail");
        }
    }
}

/// Compute scaled dimensions preserving aspect ratio, fitting within `size`.
///
/// The longer edge is scaled to `size`; the shorter edge is scaled
/// proportionally. Both dimensions are at least 1.
fn scaled_dimensions(orig_w: i32, orig_h: i32, size: i32) -> (i32, i32) {
    if orig_w >= orig_h {
        let denom = i64::from(orig_w).max(1);
        let numer = i64::from(orig_h).saturating_mul(i64::from(size));
        let raw = numer.checked_div(denom).unwrap_or(1);
        let h = raw.max(1).min(i64::from(size));
        (size, i32::try_from(h).unwrap_or(size))
    } else {
        let denom = i64::from(orig_h).max(1);
        let numer = i64::from(orig_w).saturating_mul(i64::from(size));
        let raw = numer.checked_div(denom).unwrap_or(1);
        let w = raw.max(1).min(i64::from(size));
        (i32::try_from(w).unwrap_or(size), size)
    }
}

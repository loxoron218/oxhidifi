//! Artwork extraction and caching from audio files.

use std::{
    fs::{create_dir_all, read_dir, read_to_string as fs_read_to_string, remove_file, write},
    io::ErrorKind::NotFound,
    path::{Path, PathBuf},
};

use {
    libadwaita::gtk::gdk_pixbuf::{InterpType::Bilinear, PixbufLoader, prelude::PixbufLoaderExt},
    lofty::{
        error::FileParseError,
        file::TaggedFileExt,
        picture::{MimeType, PictureType::CoverFront},
        read_from_path,
    },
    thiserror::Error,
    tracing::warn,
};

use crate::app::xdg_paths::dirs_cache_home;

/// Subdirectory for cached artwork files.
const ARTWORK_CACHE_DIR: &str = "oxhidifi/artwork";

/// File extensions to try when looking up cached artwork by key.
const ARTWORK_EXTENSIONS: &[&str] = &["jpg", "png", "webp"];

/// Current cache format version.  Bump to force re-extraction of all artwork.
const CACHE_VERSION: &str = "3";

/// Thumbnail sizes generated for grid/column views (px).
///
/// Covers the default grid (180 px) and list (48 px) sizes plus the extremes so
/// every zoom level has a close thumbnail on disk. The full-size original is
/// always cached alongside these.
const THUMBNAIL_SIZES: &[i32] = &[32, 48, 64, 120, 150, 180, 210, 240];

/// Errors occurring during artwork operations.
#[derive(Debug, Error)]
pub enum ArtworkError {
    /// Failed to read the audio file for artwork.
    #[error("Failed to read audio file for artwork: {0}")]
    ReadError(#[from] FileParseError),
    /// File not found or inaccessible.
    #[error("File not found or inaccessible: {0}")]
    FileNotFound(String),
}

/// Extract embedded artwork from an audio file.
///
/// Returns the raw bytes and the file extension (e.g., `"jpg"`, `"png"`)
/// of the first embedded picture (front cover preferred), or `None` if no
/// picture is embedded.
///
/// # Errors
///
/// Returns [`ArtworkError`] if the file cannot be read.
pub fn extract_artwork(path: &Path) -> Result<Option<(Vec<u8>, String)>, ArtworkError> {
    let tagged_file = read_from_path(path)?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let Some(tag) = tag else {
        return Ok(None);
    };

    let pictures = tag.pictures();
    if pictures.is_empty() {
        return Ok(None);
    }

    let picture = pictures
        .iter()
        .find(|p| p.pic_type() == CoverFront)
        .or_else(|| pictures.first());

    let Some(picture) = picture else {
        return Ok(None);
    };

    let ext = picture
        .mime_type()
        .and_then(MimeType::ext)
        .map_or_else(|| "png".to_string(), ToString::to_string);

    Ok(Some((picture.data().to_vec(), ext)))
}

/// Ensure the artwork cache directory exists.
///
/// # Errors
///
/// Returns an error if the directory cannot be created.
fn ensure_artwork_cache_dir() -> Result<PathBuf, ArtworkError> {
    let cache_dir = dirs_cache_home()
        .map_err(|e| ArtworkError::FileNotFound(format!("Cannot resolve XDG cache home: {e}")))?
        .join(ARTWORK_CACHE_DIR);

    create_dir_all(&cache_dir).map_err(|e| {
        ArtworkError::FileNotFound(format!(
            "Cannot create artwork cache dir {}: {e}",
            cache_dir.display()
        ))
    })?;

    Ok(cache_dir)
}

/// Cache artwork data to disk in a given cache directory and return the file path.
///
/// The artwork is stored as `{key}.{ext}`.  The extension is detected from the
/// embedded picture's MIME type (determined during extraction).
/// Downscaled thumbnails for grid/column views are generated alongside the
/// original per FR-003b and stored as `{key}_{size}.{ext}` for each size in
/// [`THUMBNAIL_SIZES`]. Thumbnail generation failures are logged as warnings
/// but do not fail the overall cache operation.
///
/// # Errors
///
/// Returns [`ArtworkError`] if the cache directory cannot be created or the
/// file cannot be written.
fn cache_artwork_in(
    cache_dir: &Path,
    key: &str,
    data: &[u8],
    ext: &str,
) -> Result<PathBuf, ArtworkError> {
    let file_path = cache_dir.join(format!("{key}.{ext}"));

    write(&file_path, data).map_err(|e| {
        ArtworkError::FileNotFound(format!(
            "Failed to write artwork cache {}: {e}",
            file_path.display()
        ))
    })?;

    generate_thumbnails(cache_dir, key, data, ext);

    Ok(file_path)
}

/// Generate downscaled thumbnails for grid/column views.
///
/// Thumbnails are written as `{key}_{size}.{ext}` for each size in
/// [`THUMBNAIL_SIZES`]. Failures are logged via `tracing::warn` and do not
/// propagate — the original artwork remains usable even if thumbnails cannot be
/// created (e.g., corrupt image bytes or missing `gdk-pixbuf` loader).
fn generate_thumbnails(cache_dir: &Path, key: &str, data: &[u8], ext: &str) {
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

/// Get the cached thumbnail path for a given key and size, returning `None` if not cached.
///
/// Tries each known extension (`.jpg`, `.png`, `.webp`) to find a matching
/// thumbnail file `{key}_{size}.{ext}`.
#[must_use]
pub fn get_cached_thumbnail_path(key: &str, size: i32) -> Option<PathBuf> {
    let Ok(cache_dir) = ensure_artwork_cache_dir() else {
        return None;
    };
    ARTWORK_EXTENSIONS
        .iter()
        .map(|ext| cache_dir.join(format!("{key}_{size}.{ext}")))
        .find(|p| p.exists())
}

/// Cache artwork data to disk and return the file path.
///
/// The artwork is stored as `{key}.{ext}` in the XDG cache artwork directory.
/// The `ext` should be one of `"jpg"`, `"png"`, or `"webp"`, determined during
/// extraction from the embedded picture's MIME type.
///
/// # Errors
///
/// Returns [`ArtworkError`] if the cache directory cannot be created or the
/// file cannot be written.
pub fn cache_artwork(key: &str, data: &[u8], ext: &str) -> Result<PathBuf, ArtworkError> {
    let cache_dir = ensure_artwork_cache_dir()?;
    cache_artwork_in(&cache_dir, key, data, ext)
}

/// Get the cached artwork path for a given key, returning `None` if not cached.
///
/// Tries each known extension (`.jpg`, `.png`, `.webp`) to find a matching
/// file, since the stored extension depends on the original embedded image
/// format.
#[must_use]
pub fn get_cached_artwork_path(key: &str) -> Option<PathBuf> {
    let Ok(cache_dir) = ensure_artwork_cache_dir() else {
        return None;
    };
    ARTWORK_EXTENSIONS
        .iter()
        .map(|ext| cache_dir.join(format!("{key}.{ext}")))
        .find(|p| p.exists())
}

/// Check and update the artwork cache version.
///
/// If the stored version does not match [`CACHE_VERSION`], the artwork
/// directory is wiped so that files are re-extracted with correctly-detected
/// MIME extensions on the next scan.
pub fn check_cache_version() {
    let Ok(cache_dir) = ensure_artwork_cache_dir() else {
        return;
    };
    let version_path = cache_dir.join(".version");

    let needs_wipe = read_to_string(&version_path).is_none_or(|v| v.trim() != CACHE_VERSION);

    if !needs_wipe {
        return;
    }

    if let Ok(entries) = read_dir(&cache_dir) {
        for path in entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.file_name().is_none_or(|n| n != ".version"))
        {
            remove_cache_file(&path);
        }
    }
    if let Err(e) = write(&version_path, CACHE_VERSION) {
        warn!(error = %e, path = %version_path.display(), "Failed to write cache version");
    }
}

/// Remove a cached artwork file, logging on failure.
fn remove_cache_file(path: &Path) {
    if let Err(e) = remove_file(path) {
        warn!(error = %e, path = %path.display(), "Failed to remove cached artwork");
    }
}

/// Read the contents of a file to a `String`, returning `None` on error.
fn read_to_string(path: &Path) -> Option<String> {
    match fs_read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == NotFound => None,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "Failed to read file");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, read},
        io::Write,
        path::Path,
    };

    use {
        anyhow::{Result, bail, ensure},
        tempfile::{NamedTempFile, tempdir},
    };

    use crate::library::artwork::{
        cache_artwork_in, extract_artwork, get_cached_artwork_path, read_to_string,
    };

    fn has_cached_artwork_in(cache_dir: &Path, key: &str) -> bool {
        ["jpg", "png", "webp"]
            .iter()
            .any(|ext| cache_dir.join(format!("{key}.{ext}")).exists())
    }

    #[test]
    fn extract_artwork_missing_file() -> Result<()> {
        let result = extract_artwork(Path::new("/nonexistent/file.flac"));
        if result.is_ok() {
            bail!("expected error for missing file");
        }
        Ok(())
    }

    #[test]
    fn extract_artwork_invalid_content_returns_error() -> Result<()> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(b"not an audio file")?;
        let result = extract_artwork(tmp.path());
        if result.is_ok() {
            bail!("expected error for invalid audio content");
        }
        Ok(())
    }

    #[test]
    fn cache_artwork_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let cache_base = dir.path().join("oxhidifi/artwork");
        create_dir_all(&cache_base)?;
        let key = "test-album-1";
        let data = b"fake-png-bytes";

        let path = cache_artwork_in(&cache_base, key, data, "png")?;
        ensure!(path.exists(), "cached file should exist");
        ensure!(read(&path)? == data, "cached data should match");
        ensure!(
            has_cached_artwork_in(&cache_base, key),
            "has_cached should be true"
        );
        Ok(())
    }

    #[test]
    fn get_cached_artwork_missing_returns_none() {
        let path = get_cached_artwork_path("nonexistent-key");
        assert!(path.is_none());
    }

    #[test]
    fn read_to_string_missing_file_returns_none() {
        let result = read_to_string(Path::new("/nonexistent/file.txt"));
        assert!(result.is_none());
    }

    #[test]
    fn read_to_string_returns_content() -> Result<()> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(b"cached version 2")?;
        let result = read_to_string(tmp.path());
        ensure!(
            result.as_deref() == Some("cached version 2"),
            "expected file content"
        );
        Ok(())
    }

    #[test]
    fn read_to_string_invalid_utf8_returns_none() -> Result<()> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(&[0xff, 0xfe, 0xfd])?;
        let result = read_to_string(tmp.path());
        ensure!(result.is_none(), "invalid UTF-8 should return None");
        Ok(())
    }
}

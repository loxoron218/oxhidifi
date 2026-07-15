//! Artwork extraction and caching from audio files.

use std::{
    fs::{create_dir_all, read_dir, read_to_string as fs_read_to_string, remove_file, write},
    io::ErrorKind::NotFound,
    path::{Path, PathBuf},
};

use {
    lofty::{
        error::LoftyError,
        file::TaggedFileExt,
        picture::{MimeType, PictureType::CoverFront},
        read_from_path,
    },
    thiserror::Error,
    tracing::warn,
};

use crate::app::dirs_cache_home;

/// Subdirectory for cached artwork files.
const ARTWORK_CACHE_DIR: &str = "oxhidifi/artwork";

/// File extensions to try when looking up cached artwork by key.
const ARTWORK_EXTENSIONS: &[&str] = &["jpg", "png", "webp"];

/// Current cache format version.  Bump to force re-extraction of all artwork.
const CACHE_VERSION: &str = "2";

/// Errors occurring during artwork operations.
#[derive(Debug, Error)]
pub enum ArtworkError {
    /// Failed to read the audio file for artwork.
    #[error("Failed to read audio file for artwork: {0}")]
    ReadError(#[from] LoftyError),
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
        .map_or("png".to_string(), ToString::to_string);

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

    Ok(file_path)
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

    use crate::library::artwork::{cache_artwork_in, extract_artwork, get_cached_artwork_path};

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
}

//! Artwork extraction and caching from audio files.

use std::{
    fs::{create_dir_all, write},
    path::{Path, PathBuf},
};

use {
    lofty::{
        error::LoftyError, file::TaggedFileExt, picture::PictureType::CoverFront, read_from_path,
    },
    thiserror::Error,
};

use crate::app::dirs_cache_home;

/// Subdirectory for cached artwork files.
const ARTWORK_CACHE_DIR: &str = "oxhidifi/artwork";

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
/// Returns the raw bytes of the first embedded picture (front cover preferred),
/// or `None` if no picture is embedded.
///
/// # Errors
///
/// Returns [`ArtworkError`] if the file cannot be read.
pub fn extract_artwork(path: &Path) -> Result<Option<Vec<u8>>, ArtworkError> {
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

    // Prefer front cover, otherwise use the first picture.
    let picture = pictures
        .iter()
        .find(|p| p.pic_type() == CoverFront)
        .or_else(|| pictures.first());

    picture.map(|p| Ok(p.data().to_vec())).transpose()
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
/// The artwork is stored as a PNG file named `{key}.png`.
///
/// # Errors
///
/// Returns [`ArtworkError`] if the cache directory cannot be created or the
/// file cannot be written.
fn cache_artwork_in(cache_dir: &Path, key: &str, data: &[u8]) -> Result<PathBuf, ArtworkError> {
    let file_path = cache_dir.join(format!("{key}.png"));

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
/// The artwork is stored as a PNG file named `{key}` in the XDG cache
/// artwork directory. The `key` should uniquely identify the album (e.g.,
/// its database ID as a string).
///
/// # Errors
///
/// Returns [`ArtworkError`] if the cache directory cannot be created or the
/// file cannot be written.
pub fn cache_artwork(key: &str, data: &[u8]) -> Result<PathBuf, ArtworkError> {
    let cache_dir = ensure_artwork_cache_dir()?;
    cache_artwork_in(&cache_dir, key, data)
}

/// Check whether a cached artwork file exists for a given key in the given
/// cache directory.
fn has_cached_artwork_in(cache_dir: &Path, key: &str) -> bool {
    cache_dir.join(format!("{key}.png")).exists()
}

/// Get the cached artwork path for a given key, returning `None` if not cached.
#[must_use]
pub fn get_cached_artwork_path(key: &str) -> Option<PathBuf> {
    let cache_dir = ensure_artwork_cache_dir().ok()?;
    let file_path = cache_dir.join(format!("{key}.png"));
    file_path.exists().then_some(file_path)
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
        cache_artwork_in, extract_artwork, get_cached_artwork_path, has_cached_artwork_in,
    };

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

        let path = cache_artwork_in(&cache_base, key, data)?;
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

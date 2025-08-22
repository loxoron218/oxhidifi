use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
    fs::create_dir_all,
    io,
    path::PathBuf,
};

use gdk_pixbuf::{InterpType::Bilinear, PixbufLoader, prelude::PixbufLoaderExt};
use glib::user_cache_dir;
use tokio::fs::write;

use crate::utils::image_cache::ThumbnailError::{CacheDir, Close, Empty, Load, Save};

const THUMBNAIL_SIZE: i32 = 512;

#[derive(Debug)]
pub enum ThumbnailError {
    CacheDir(io::Error),
    Load(glib::Error),
    Close,
    Empty,
    Save(glib::Error),
}

impl Display for ThumbnailError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            CacheDir(e) => write!(f, "Failed to create cache directory: {}", e),
            Load(e) => write!(f, "Failed to load image data: {}", e),
            Close => write!(f, "Failed to close image loader"),
            Empty => write!(f, "Pixbuf is empty after loading"),
            Save(e) => write!(f, "Failed to save thumbnail: {}", e),
        }
    }
}

impl Error for ThumbnailError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CacheDir(e) => Some(e),
            Load(e) => Some(e),
            Save(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ThumbnailError {
    fn from(err: io::Error) -> ThumbnailError {
        CacheDir(err)
    }
}

/// Returns the path to the album art cache directory, creating it if it doesn't exist.
fn get_or_create_cache_dir() -> Result<PathBuf, io::Error> {
    let mut cache_dir = user_cache_dir();
    cache_dir.push("oxhidifi");
    cache_dir.push("covers");
    create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}

/// Generates a sanitized, unique filename for a cached thumbnail based on album details.
fn generate_cache_filename(album_title: &str, album_artist_name: &str) -> String {
    let mut name = String::with_capacity(album_artist_name.len() + album_title.len() + 1);
    name.push_str(album_artist_name);
    name.push('-');
    name.push_str(album_title);

    // Sanitize the filename to remove characters that are invalid on most filesystems.
    let sanitized: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("{}.jpg", sanitized)
}

/// Processes raw image data, resizes it, and saves it as a thumbnail in the cache.
///
/// If a thumbnail for the given album already exists in the cache, it returns the path directly.
/// Otherwise, it creates a thumbnail from the raw data, saves it, and returns the new path.
/// This function is `async` because it performs file I/O to save the thumbnail.
///
/// # Arguments
/// * `image_data` - A slice of bytes representing the raw image data from the audio file.
/// * `album_title` - The title of the album.
/// * `album_artist_name` - The name of the album's artist.
///
/// # Returns
/// A `Result` containing the `PathBuf` to the cached thumbnail, or a `ThumbnailError`.
pub async fn get_or_create_thumbnail(
    image_data: &[u8],
    album_title: &str,
    album_artist_name: &str,
) -> Result<PathBuf, ThumbnailError> {
    let cache_dir = get_or_create_cache_dir()?;
    let filename = generate_cache_filename(album_title, album_artist_name);
    let cache_path = cache_dir.join(filename);

    // If the thumbnail already exists, no need to process it again.
    if cache_path.exists() {
        return Ok(cache_path);
    }

    // The image processing part is synchronous as it operates on data already in memory.
    let loader = PixbufLoader::new();
    loader.write(image_data).map_err(Load)?;
    loader.close().map_err(|_| Close)?;
    let pixbuf = loader.pixbuf().ok_or(Empty)?;

    // Scale the image to the desired thumbnail size.
    let scaled_pixbuf = pixbuf
        .scale_simple(THUMBNAIL_SIZE, THUMBNAIL_SIZE, Bilinear)
        .ok_or(Empty)?; // Handle case where scaling might fail

    // Save the created thumbnail to the cache directory as a JPEG.
    // We do this in-memory first to prepare the bytes for async writing.
    let buffer = scaled_pixbuf
        .save_to_bufferv("jpeg", &[("quality", "90")])
        .map_err(Save)?;

    // The file I/O part is asynchronous using tokio.
    write(&cache_path, &buffer).await?;
    Ok(cache_path)
}

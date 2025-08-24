use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
    fs::create_dir_all,
    io,
    path::PathBuf,
};

use fast_image_resize::{
    FilterType::Lanczos3, ImageBufferError, PixelType::U8x4, ResizeAlg, ResizeError, ResizeOptions,
    Resizer, images::Image,
};
use glib::user_cache_dir;
use image::{
    ImageError,
    ImageFormat::Jpeg,
    RgbaImage,
    error::{ParameterError, ParameterErrorKind::DimensionMismatch},
    load_from_memory,
};
use tokio::fs::write;

use crate::utils::image_cache::ThumbnailError::{CacheDir, ImageBuffer, Load, Resize};

const THUMBNAIL_SIZE: i32 = 512;

#[derive(Debug)]
pub enum ThumbnailError {
    CacheDir(io::Error),
    /// An error occurred while loading or processing the image data
    Load(ImageError),
    /// An error occurred during fast_image_resize operations
    Resize(ResizeError),
    /// An error occurred during fast_image_resize image buffer operations
    ImageBuffer(ImageBufferError),
}

/// Implementation of Display trait for ThumbnailError
///
/// This implementation provides user-friendly error messages for all error variants.
impl Display for ThumbnailError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            CacheDir(e) => write!(f, "Failed to create cache directory: {}", e),
            Load(e) => write!(f, "Failed to load image data: {}", e),
            Resize(e) => write!(f, "Failed to resize image: {}", e),
            ImageBuffer(e) => write!(f, "Failed to create image buffer: {}", e),
        }
    }
}

impl Error for ThumbnailError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CacheDir(e) => Some(e),
            Load(e) => Some(e),
            Resize(e) => Some(e),
            ImageBuffer(e) => Some(e),
        }
    }
}

impl From<io::Error> for ThumbnailError {
    fn from(err: io::Error) -> ThumbnailError {
        CacheDir(err)
    }
}

impl From<image::ImageError> for ThumbnailError {
    fn from(err: image::ImageError) -> ThumbnailError {
        Load(err)
    }
}

/// Implementation of From trait to convert fast_image_resize::ResizeError to ThumbnailError
///
/// This implementation allows fast_image_resize::ResizeError to be automatically converted to
/// ThumbnailError::Resize variant when using the ? operator.
impl From<ResizeError> for ThumbnailError {
    fn from(err: ResizeError) -> ThumbnailError {
        Resize(err)
    }
}

/// Implementation of From trait to convert fast_image_resize::ImageBufferError to ThumbnailError
///
/// This implementation allows fast_image_resize::ImageBufferError to be automatically converted to
/// ThumbnailError::ImageBuffer variant when using the ? operator.
impl From<ImageBufferError> for ThumbnailError {
    fn from(err: ImageBufferError) -> ThumbnailError {
        ImageBuffer(err)
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
    // Use the image crate for better performance and quality
    let img = image::load_from_memory(image_data)?;

    // Convert to RGBA8 format for fast_image_resize
    let rgba_img = img.to_rgba8();
    let src_width = rgba_img.width();
    let src_height = rgba_img.height();

    // Create source and destination image views for fast_image_resize
    let src_image = Image::from_vec_u8(src_width, src_height, rgba_img.into_raw(), U8x4)?;

    let dst_width = THUMBNAIL_SIZE as u32;
    let dst_height = THUMBNAIL_SIZE as u32;
    let mut dst_image = Image::new(dst_width, dst_height, U8x4);

    // Create resizer and resize the image
    let mut resizer = Resizer::new();
    let resize_options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(Lanczos3));
    resizer.resize(&src_image, &mut dst_image, &resize_options)?;

    // Convert back to image::RgbaImage for JPEG encoding
    let resized_rgba_img = RgbaImage::from_raw(dst_width, dst_height, dst_image.into_vec()).ok_or(
        ImageError::Parameter(ParameterError::from_kind(DimensionMismatch)),
    )?;

    // Encode as JPEG with quality 90
    let mut buffer: Vec<u8> = Vec::new();
    resized_rgba_img.write_to(&mut Cursor::new(&mut buffer), Jpeg)?;

    // The file I/O part is asynchronous using tokio.
    write(&cache_path, &buffer).await?;
    Ok(cache_path)
}

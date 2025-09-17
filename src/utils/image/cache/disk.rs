use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use gdk_pixbuf::Pixbuf;
use gtk4::glib::{FileError::Noent, user_cache_dir};

use crate::utils::image::{cache::memory::hash_to_hex, error::ImageLoaderError};

/// Disk cache for scaled images
///
/// This cache stores scaled images on disk to avoid reprocessing the same
/// images multiple times. It uses a hash-based naming scheme to ensure
/// unique filenames for different image paths and sizes.
pub struct DiskCache {
    /// The directory where cached images are stored
    cache_dir: PathBuf,
}

impl DiskCache {
    /// Creates a new disk cache
    ///
    /// This method initializes the disk cache by creating the cache directory
    /// in the user's cache directory. The cache directory is located at
    /// `~/.cache/oxhidifi/scaled_covers` on Unix-like systems.
    ///
    /// # Returns
    /// A `Result` containing the new `DiskCache` instance or an `ImageLoaderError`
    pub fn new() -> Result<Self, ImageLoaderError> {
        let mut cache_dir = user_cache_dir();
        cache_dir.push("oxhidifi");
        cache_dir.push("scaled_covers");

        // Create the directory if it doesn't exist
        create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Generates a cache path for an image
    ///
    /// This method creates a unique filename for a cached image based on
    /// the original image path and the desired size. The filename is a
    /// hash of the original path combined with the size to ensure uniqueness.
    ///
    /// # Arguments
    /// * `original_path` - The path to the original image file
    /// * `size` - The size (width and height) of the scaled image
    ///
    /// # Returns
    /// The path where the cached image should be stored
    pub fn get_cache_path(&self, original_path: &Path, size: i32) -> PathBuf {
        // Create a hash-based filename to ensure uniqueness
        let filename = format!(
            "{}_{}.jpg",
            hash_to_hex(original_path.to_string_lossy().as_bytes()),
            size
        );
        self.cache_dir.join(filename)
    }

    /// Loads an image from the disk cache
    ///
    /// This method attempts to load a scaled image from the disk cache.
    /// If the image exists in the cache, it is loaded and returned.
    /// If the image is not in the cache, `Ok(None)` is returned.
    ///
    /// # Arguments
    /// * `original_path` - The path to the original image file
    /// * `size` - The size (width and height) of the scaled image
    ///
    /// # Returns
    /// A `Result` containing `Some(pixbuf)` if the image is in the cache,
    /// `None` if it's not in the cache, or an `ImageLoaderError` if an error occurred
    pub fn load(
        &self,
        original_path: &Path,
        size: i32,
    ) -> Result<Option<Pixbuf>, ImageLoaderError> {
        let cache_path = self.get_cache_path(original_path, size);
        match Pixbuf::from_file_at_scale(&cache_path, size, size, true) {
            Ok(pixbuf) => Ok(Some(pixbuf)),
            Err(e) => {
                // Check if the error is specifically a "file not found" error.
                // The exact method may vary slightly based on the error type,
                // but for glib-based errors, this is a common pattern.
                if e.matches(Noent) {
                    // This is the expected "cache miss" case, not a real error.
                    Ok(None)
                } else {
                    // This is a real error (e.g., corrupt file, permissions issue).
                    // Propagate it up.
                    Err(e.into())
                }
            }
        }
    }

    /// Saves an image to the disk cache
    ///
    /// This method saves a scaled image to the disk cache for future use.
    /// The image is saved as a JPEG with quality 90.
    ///
    /// # Arguments
    /// * `original_path` - The path to the original image file
    /// * `size` - The size (width and height) of the scaled image
    /// * `pixbuf` - The image data to save
    ///
    /// # Returns
    /// A `Result` indicating success or an `ImageLoaderError` if an error occurred
    pub fn save(
        &self,
        original_path: &Path,
        size: i32,
        pixbuf: &Pixbuf,
    ) -> Result<(), ImageLoaderError> {
        let cache_path = self.get_cache_path(original_path, size);

        // Save the pixbuf to the cache path
        pixbuf.savev(&cache_path, "jpeg", &[("quality", "90")])?;
        Ok(())
    }
}

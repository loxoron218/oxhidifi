use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
    fs::create_dir_all,
    hash::Hasher,
    io::{self, Cursor},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Instant,
};

use fast_image_resize::{
    FilterType::Lanczos3, ImageBufferError, PixelType::U8x4, ResizeAlg::Convolution, ResizeError,
    ResizeOptions, Resizer, images,
};
use gdk_pixbuf::{Pixbuf, PixbufLoader};
use glib::user_cache_dir;
use image::{
    ImageError::{self, Parameter},
    ImageFormat::Jpeg,
    RgbaImage,
    error::{ParameterError, ParameterErrorKind::DimensionMismatch},
};
use libadwaita::prelude::PixbufLoaderExt;

use crate::utils::image_loader::ImageLoaderError::{
    Glib, Image, ImageBuffer, InvalidPath, Io, Resize,
};

/// Generates a hex string from the hash of the input bytes using DefaultHasher
fn hash_to_hex(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    let hash_value = hasher.finish();
    format!("{:x}", hash_value)
}

/// Error types for the image loader
///
/// This enum represents all possible errors that can occur during image loading operations.
/// It provides a unified error type that can be used throughout the image loading pipeline.
#[derive(Debug)]
pub enum ImageLoaderError {
    /// An I/O error occurred (e.g., file not found, permission denied)
    Io(io::Error),
    /// An image processing error occurred (e.g., unsupported format, corrupted data)
    Image(ImageError),
    /// A GLib error occurred (e.g., during pixbuf operations)
    Glib(glib::Error),
    /// The image path was invalid or the pixbuf could not be created
    InvalidPath,
    /// An error occurred during fast_image_resize operations
    Resize(ResizeError),
    /// An error occurred during fast_image_resize image buffer operations
    ImageBuffer(ImageBufferError),
}

/// Implementation of Display trait for ImageLoaderError
///
/// This implementation provides user-friendly error messages for all error variants.
impl Display for ImageLoaderError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Io(e) => write!(f, "IO error: {}", e),
            Image(e) => write!(f, "Image error: {}", e),
            Glib(e) => write!(f, "GLib error: {}", e),
            InvalidPath => write!(f, "Invalid path"),
            Resize(e) => write!(f, "Resize error: {}", e),
            ImageBuffer(e) => write!(f, "Image buffer error: {}", e),
        }
    }
}

/// Implementation of Error trait for ImageLoaderError
///
/// This implementation allows ImageLoaderError to be used as a standard error type
/// and provides access to the underlying error source when available.
impl Error for ImageLoaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Io(e) => Some(e),
            Image(e) => Some(e),
            Glib(e) => Some(e),
            Resize(e) => Some(e),
            ImageBuffer(e) => Some(e),
            InvalidPath => None,
        }
    }
}

/// Implementation of From trait to convert io::Error to ImageLoaderError
///
/// This implementation allows io::Error to be automatically converted to
/// ImageLoaderError::Io variant when using the ? operator.
impl From<io::Error> for ImageLoaderError {
    fn from(err: io::Error) -> Self {
        Io(err)
    }
}

/// Implementation of From trait to convert ImageError to ImageLoaderError
///
/// This implementation allows ImageError to be automatically converted to
/// ImageLoaderError::Image variant when using the ? operator.
impl From<ImageError> for ImageLoaderError {
    fn from(err: ImageError) -> Self {
        Image(err)
    }
}

/// Implementation of From trait to convert fast_image_resize::ResizeError to ImageLoaderError
///
/// This implementation allows fast_image_resize::ResizeError to be automatically converted to
/// ImageLoaderError::Resize variant when using the ? operator.
impl From<ResizeError> for ImageLoaderError {
    fn from(err: ResizeError) -> Self {
        Resize(err)
    }
}

/// Implementation of From trait to convert fast_image_resize::ImageBufferError to ImageLoaderError
///
/// This implementation allows fast_image_resize::ImageBufferError to be automatically converted to
/// ImageLoaderError::ImageBuffer variant when using the ? operator.
impl From<ImageBufferError> for ImageLoaderError {
    fn from(err: ImageBufferError) -> Self {
        ImageBuffer(err)
    }
}

/// Implementation of From trait to convert glib::Error to ImageLoaderError
///
/// This implementation allows glib::Error to be automatically converted to
/// ImageLoaderError::Glib variant when using the ? operator.
impl From<glib::Error> for ImageLoaderError {
    fn from(err: glib::Error) -> Self {
        Glib(err)
    }
}

/// Memory cache entry
///
/// Represents a single entry in the memory cache, storing both the pixbuf
/// and metadata about when it was last accessed for LRU eviction.
#[derive(Clone)]
struct CacheEntry {
    /// The cached image data
    pixbuf: Pixbuf,
    /// Timestamp of the last access for LRU eviction
    last_access: Instant,
}

/// Memory cache implementation using LRU eviction policy
///
/// This cache stores recently accessed images in memory for fast retrieval.
/// It uses a Least Recently Used (LRU) eviction policy to manage memory usage,
/// automatically removing the least recently accessed entries when the cache
/// exceeds its maximum capacity.
struct MemoryCache {
    /// Thread-safe storage for cache entries
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Maximum number of entries to store in the cache
    max_entries: usize,
}

impl MemoryCache {
    /// Creates a new memory cache with the specified maximum capacity
    ///
    /// # Arguments
    /// * `max_entries` - The maximum number of entries to store in the cache
    ///
    /// # Returns
    /// A new `MemoryCache` instance
    fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    /// Retrieves an image from the cache if it exists
    ///
    /// If the image exists in the cache, this method updates its last access
    /// timestamp and returns a clone of the pixbuf. If the image is not in
    /// the cache, it returns `None`.
    ///
    /// # Arguments
    /// * `key` - The cache key to look up
    ///
    /// # Returns
    /// `Some(pixbuf)` if the image is in the cache, `None` otherwise
    fn get(&self, key: &str) -> Option<Pixbuf> {
        let mut entries = self.entries.write().unwrap();
        if let Some(entry) = entries.get_mut(key) {
            entry.last_access = Instant::now();
            Some(entry.pixbuf.clone())
        } else {
            None
        }
    }

    /// Inserts an image into the cache
    ///
    /// This method adds a new image to the cache and performs LRU eviction
    /// if necessary. If the cache exceeds its maximum capacity after insertion,
    /// the least recently used entries are removed.
    ///
    /// # Arguments
    /// * `key` - The cache key for the image
    /// * `pixbuf` - The image data to cache
    fn insert(&self, key: String, pixbuf: Pixbuf) {
        let mut entries = self.entries.write().unwrap();

        // Insert the new entry
        entries.insert(
            key,
            CacheEntry {
                pixbuf,
                last_access: Instant::now(),
            },
        );

        // Evict oldest entries if we exceed the limit
        if entries.len() > self.max_entries {
            // Collect entries and sort by last access time
            let mut entries_vec: Vec<_> = entries.iter().collect();
            entries_vec.sort_by_key(|(_, entry)| entry.last_access);

            // Calculate how many entries to remove
            let to_remove = entries.len() - self.max_entries;

            // Collect keys to remove
            let keys_to_remove: Vec<_> = entries_vec
                .into_iter()
                .take(to_remove)
                .map(|(key, _)| key.clone())
                .collect();

            // Remove the oldest entries
            for key in keys_to_remove {
                entries.remove(&key);
            }
        }
    }
}

/// Disk cache for scaled images
///
/// This cache stores scaled images on disk to avoid reprocessing the same
/// images multiple times. It uses a hash-based naming scheme to ensure
/// unique filenames for different image paths and sizes.
struct DiskCache {
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
    fn new() -> Result<Self, ImageLoaderError> {
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
    fn get_cache_path(&self, original_path: &Path, size: i32) -> PathBuf {
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
    fn load(&self, original_path: &Path, size: i32) -> Result<Option<Pixbuf>, ImageLoaderError> {
        let cache_path = self.get_cache_path(original_path, size);

        if cache_path.exists() {
            // Load from cache
            let pixbuf = Pixbuf::from_file_at_scale(&cache_path, size, size, true)?;
            Ok(Some(pixbuf))
        } else {
            Ok(None)
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
    fn save(
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

/// Main image loader that handles async loading with caching
///
/// This struct provides the core functionality for loading and caching images.
/// It combines memory and disk caching to provide optimal performance while
/// managing memory usage. The loader uses a two-tier caching strategy:
///
/// 1. Memory cache (LRU) for recently accessed images
/// 2. Disk cache for scaled images to avoid reprocessing
///
/// The loader also handles image scaling using high-quality Lanczos filtering
/// and converts images to the appropriate format for GTK widgets.
pub struct ImageLoader {
    /// Memory cache for recently accessed images
    memory_cache: MemoryCache,
    /// Disk cache for scaled images
    disk_cache: DiskCache,
}

impl ImageLoader {
    /// Creates a new image loader with default cache settings
    ///
    /// This method initializes both the memory cache (with 200 entries capacity)
    /// and the disk cache. It will return an error if the disk cache cannot
    /// be initialized (e.g., if the cache directory cannot be created).
    ///
    /// # Returns
    /// A `Result` containing the new `ImageLoader` instance or an `ImageLoaderError`
    pub fn new() -> Result<Self, ImageLoaderError> {
        Ok(Self {
            memory_cache: MemoryCache::new(200),
            disk_cache: DiskCache::new()?,
        })
    }

    /// Load an image with caching
    ///
    /// This method loads an image from the specified path, scales it to the
    /// desired size, and caches it in both memory and disk caches. The loading
    /// process follows this priority order:
    ///
    /// 1. Check memory cache first (fastest)
    /// 2. Check disk cache second (faster than reprocessing)
    /// 3. Load and process original image (slowest)
    ///
    /// # Arguments
    /// * `path` - The path to the image file to load
    /// * `size` - The size (width and height) to scale the image to
    ///
    /// # Returns
    /// A `Result` containing the loaded and scaled `Pixbuf` or an `ImageLoaderError`
    pub fn load_image(&self, path: &Path, size: i32) -> Result<Pixbuf, ImageLoaderError> {
        // Generate cache key
        let cache_key = format!(
            "{}_{}",
            hash_to_hex(path.to_string_lossy().as_bytes()),
            size
        );

        // 1. Check memory cache
        if let Some(pixbuf) = self.memory_cache.get(&cache_key) {
            return Ok(pixbuf);
        }

        // 2. Check disk cache
        if let Some(pixbuf) = self.disk_cache.load(path, size)? {
            // Store in memory cache
            self.memory_cache.insert(cache_key, pixbuf.clone());
            return Ok(pixbuf);
        }

        // 3. Load and scale the original image using fast_image_resize
        let img = image::open(path)?;

        // Convert to RGBA8 format for fast_image_resize
        let rgba_img = img.to_rgba8();
        let src_width = rgba_img.width();
        let src_height = rgba_img.height();

        // Create source and destination image views for fast_image_resize
        let src_image =
            images::Image::from_vec_u8(src_width, src_height, rgba_img.into_raw(), U8x4)?;

        let dst_width = size as u32;
        let dst_height = size as u32;
        let mut dst_image = images::Image::new(dst_width, dst_height, U8x4);

        // Create resizer and resize the image
        let mut resizer = Resizer::new();
        let resize_options = ResizeOptions::new().resize_alg(Convolution(Lanczos3));
        resizer.resize(&src_image, &mut dst_image, &resize_options)?;

        // Convert back to image::RgbaImage for JPEG encoding
        let resized_rgba_img = RgbaImage::from_raw(dst_width, dst_height, dst_image.into_vec())
            .ok_or(Parameter(ParameterError::from_kind(DimensionMismatch)))?;

        // Convert to Pixbuf
        let mut buffer: Vec<u8> = Vec::new();
        resized_rgba_img.write_to(&mut Cursor::new(&mut buffer), Jpeg)?;
        let loader = PixbufLoader::new();
        loader.write(&buffer)?;
        loader.close()?;
        let pixbuf = loader.pixbuf().ok_or(InvalidPath)?;

        // 4. Save to disk cache
        if let Err(e) = self.disk_cache.save(path, size, &pixbuf) {
            eprintln!("Failed to save image to disk cache: {}", e);
        }

        // 5. Store in memory cache
        self.memory_cache.insert(cache_key, pixbuf.clone());
        Ok(pixbuf)
    }
}

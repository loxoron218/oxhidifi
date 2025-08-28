use std::{
    collections::hash_map::DefaultHasher,
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
    fs::create_dir_all,
    hash::Hasher,
    io::{self, Cursor},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use gdk_pixbuf::{Pixbuf, PixbufLoader};
use glib::user_cache_dir;
use image::{
    ImageError,
    ImageFormat::Jpeg,
    imageops::FilterType::{CatmullRom, Lanczos3, Triangle},
};
use libadwaita::prelude::PixbufLoaderExt;
use lru::LruCache;

use crate::utils::image_loader::ImageLoaderError::{Glib, Image, InvalidPath, Io};

/// Generates a hex string from the hash of the input bytes using DefaultHasher
pub fn hash_to_hex(bytes: &[u8]) -> String {
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
        }
    }
}

/// Implementation of Error trait for ImageLoaderError
///
/// This implementation allows ImageLoaderError to be used as a standard error type
/// and provides access to the underlying error source when available.
impl Error for ImageLoaderError {}

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
/// and metadata about expiration and size.
#[derive(Clone)]
struct CacheEntry {
    /// The cached image data
    pixbuf: Pixbuf,
    /// Expiration time for the cache entry
    expires_at: Instant,
    /// Estimated size of the entry in bytes
    size: usize,
}

/// Memory cache implementation using LRU eviction policy
///
/// This cache stores recently accessed images in memory for fast retrieval.
/// It uses a Least Recently Used (LRU) eviction policy to manage memory usage,
/// automatically removing the least recently accessed entries when the cache
/// exceeds its maximum capacity or size limits.
///
/// This implementation uses the `lru` crate for efficient O(1) operations
/// and tracks total cache size incrementally for O(1) size checks.
struct MemoryCache {
    /// Thread-safe LRU cache storage
    cache: Arc<RwLock<LruCache<String, CacheEntry>>>,
    /// Maximum total size of entries in the cache (in bytes)
    max_size: usize,
    /// Current total size of all entries in the cache
    total_size: Arc<RwLock<usize>>,
    /// Time-to-live for cache entries
    ttl: Duration,
}

impl MemoryCache {
    /// Creates a new memory cache with the specified maximum capacity
    ///
    /// # Arguments
    /// * `max_entries` - The maximum number of entries to store in the cache
    /// * `max_size` - The maximum total size of entries in the cache (in bytes)
    /// * `ttl` - Time-to-live for cache entries
    ///
    /// # Returns
    /// A new `MemoryCache` instance
    fn new(max_entries: usize, max_size: usize, ttl: Duration) -> Self {
        // Create an LRU cache with the specified maximum entries
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(max_entries).unwrap(),
            ))),
            max_size,
            total_size: Arc::new(RwLock::new(0)),
            ttl,
        }
    }

    /// Retrieves an image from the cache if it exists and hasn't expired
    ///
    /// If the image exists in the cache and hasn't expired, this method promotes
    /// the entry to most recently used and returns a clone of the pixbuf.
    /// If the image is not in the cache or has expired, it returns `None`.
    ///
    /// # Arguments
    /// * `key` - The cache key to look up
    ///
    /// # Returns
    /// `Some(pixbuf)` if the image is in the cache and hasn't expired, `None` otherwise
    fn get(&self, key: &str) -> Option<Pixbuf> {
        let mut cache = self.cache.write().unwrap();
        let now = Instant::now();

        // Check if the entry exists and hasn't expired
        if let Some(entry) = cache.get(key) {
            if entry.expires_at > now {
                // Entry is valid, return a clone of the pixbuf
                // The get() operation already promoted it to most recently used
                return Some(entry.pixbuf.clone());
            }

            // Entry is expired, fall through to return None
        }

        // Remove expired entry if it exists
        if cache.contains(key) {
            cache.pop(key);
        }

        // Return None if the entry was not found or was expired
        None
    }

    /// Evicts entries from the cache until the total size is within limits
    ///
    /// This method efficiently removes the least recently used entries
    /// until the total cache size is within the maximum allowed size.
    ///
    /// Time complexity: O(M) where M is the number of entries evicted
    fn evict_until_within_limit(&self) {
        let mut cache = self.cache.write().unwrap();
        let mut total_size = self.total_size.write().unwrap();
        while *total_size > self.max_size && !cache.is_empty() {
            // Efficiently remove the least recently used entry
            if let Some((_, entry)) = cache.pop_lru() {
                *total_size -= entry.size;
            } else {
                break;
            }
        }
    }

    /// Inserts an image into the cache
    ///
    /// This method stores an image in the cache with the specified key.
    /// It also performs LRU eviction and size limit enforcement.
    ///
    /// # Arguments
    /// * `key` - The cache key to store the image under
    /// * `pixbuf` - The image data to store
    fn insert(&self, key: String, pixbuf: Pixbuf) {
        let now = Instant::now();

        // Estimate size: width * height * 4 bytes per pixel
        let size = (pixbuf.width() as usize) * (pixbuf.height() as usize) * 4;

        // Create new entry
        let entry = CacheEntry {
            pixbuf,
            expires_at: now + self.ttl,
            size,
        };

        {
            let mut cache = self.cache.write().unwrap();
            let mut total_size = self.total_size.write().unwrap();

            // If an entry with this key already exists, subtract its size from total
            if let Some(old_entry) = cache.pop(&key) {
                *total_size -= old_entry.size;
            }

            // Add the new entry size to total
            *total_size += size;

            // Insert the new entry (this automatically promotes it to most recently used)
            cache.put(key, entry);
        }

        // Perform size-based eviction if we exceed max_size
        self.evict_until_within_limit();
    }
}

// Implement Clone manually for MemoryCache since LruCache doesn't implement Clone
impl Clone for MemoryCache {
    fn clone(&self) -> Self {
        // For cloning, we create a new empty cache with the same parameters
        // This is a reasonable approach since cloning a cache is rarely needed
        // and recreating it is simpler than deep cloning all entries
        let cache_size = self.cache.read().unwrap().cap();

        // Clone implementation creates a new empty cache with the same parameters
        // This is a shallow clone that doesn't copy the actual cache entries
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            max_size: self.max_size,
            total_size: Arc::new(RwLock::new(0)),
            ttl: self.ttl,
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
        match Pixbuf::from_file_at_scale(&cache_path, size, size, true) {
            Ok(pixbuf) => Ok(Some(pixbuf)),
            Err(e) => {
                // Check if the error is specifically a "file not found" error.
                // The exact method may vary slightly based on the error type,
                // but for glib-based errors, this is a common pattern.
                if e.matches(glib::FileError::Noent) {
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
    /// This method initializes both the memory cache (with 200 entries capacity,
    /// 50MB max size, and 5 minute TTL) and the disk cache. It will return an
    /// error if the disk cache cannot be initialized (e.g., if the cache directory
    /// cannot be created).
    ///
    /// # Returns
    /// A `Result` containing the new `ImageLoader` instance or an `ImageLoaderError`
    pub fn new() -> Result<Self, ImageLoaderError> {
        Ok(Self {
            memory_cache: MemoryCache::new(200, 50 * 1024 * 1024, Duration::from_secs(300)), // 50MB, 5 minutes
            disk_cache: DiskCache::new()?,
        })
    }

    /// Load an image with adaptive resizing based on target size
    pub fn load_image_adaptive(&self, path: &Path, size: i32) -> Result<Pixbuf, ImageLoaderError> {
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
            self.memory_cache.insert(cache_key.clone(), pixbuf.clone());
            return Ok(pixbuf);
        }

        // 3. Load and scale the original image with adaptive filtering
        let img = image::open(path)?;

        // Use different filter types based on size
        let filter_type = if size <= 128 {
            // For small thumbnails, use faster bilinear filtering
            Triangle
        } else if size <= 256 {
            // For medium thumbnails, use Catmull-Rom (good balance of quality and speed)
            CatmullRom
        } else {
            // For larger images, use high-quality Lanczos filtering
            Lanczos3
        };
        let scaled_img = img.resize(size as u32, size as u32, filter_type);

        // Convert to RGB if necessary
        let rgb_img = scaled_img.to_rgb8();

        // Convert to Pixbuf
        let mut buffer: Vec<u8> = Vec::new();
        rgb_img.write_to(&mut Cursor::new(&mut buffer), Jpeg)?;
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

use std::{io::Cursor, path::Path, time::Duration};

use gtk4::gdk_pixbuf::{Pixbuf, PixbufLoader};
use image::{
    ImageFormat::Jpeg,
    ImageReader,
    imageops::FilterType::{CatmullRom, Lanczos3, Triangle},
};
use libadwaita::prelude::PixbufLoaderExt;

use crate::utils::image::{
    ImageLoaderError::InvalidPath,
    cache::{
        disk::DiskCache,
        memory::{MemoryCache, hash_to_hex},
    },
    error::ImageLoaderError,
};

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
            // 50MB, 5 minutes
            memory_cache: MemoryCache::new(200, 50 * 1024 * 1024, Duration::from_secs(300)),
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
        } else {
            println!("Memory cache miss for: {} (size: {})", path.display(), size);
        }

        // 2. Check disk cache
        if let Some(pixbuf) = self.disk_cache.load(path, size)? {
            // Store in memory cache
            self.memory_cache.insert(cache_key.clone(), pixbuf.clone());
            return Ok(pixbuf);
        } else {
            println!("Disk cache miss for: {} (size: {})", path.display(), size);
        }

        // 3. Load and scale the original image with adaptive filtering
        let img = ImageReader::open(path)?.decode()?;

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
        let rgb_img = scaled_img.to_rgb8();

        // Convert to Pixbuf
        let mut buffer: Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        rgb_img.write_to(&mut cursor, Jpeg)?;
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

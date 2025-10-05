use std::{
    cell::RefCell,
    collections::hash_map::DefaultHasher,
    hash::Hasher,
    num::NonZeroUsize,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk4::gdk_pixbuf::Pixbuf;

use crate::utils::image::cache::lru::LruCache;

/// Generates a hex string from the hash of the input bytes using DefaultHasher
pub fn hash_to_hex(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    let hash_value = hasher.finish();
    format!("{:x}", hash_value)
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
pub struct MemoryCache {
    /// Thread-local LRU cache storage (not thread-safe, use from main thread only)
    cache: Rc<RefCell<LruCache<String, CacheEntry>>>,
    /// Maximum total size of entries in the cache (in bytes)
    max_size: usize,
    /// Current total size of all entries in the cache
    total_size: Rc<RefCell<usize>>,
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
    pub fn new(max_entries: usize, max_size: usize, ttl: Duration) -> Self {
        // Create an LRU cache with the specified maximum entries
        Self {
            cache: Rc::new(RefCell::new(LruCache::new(
                NonZeroUsize::new(max_entries).unwrap(),
            ))),
            max_size,
            total_size: Rc::new(RefCell::new(0)),
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
    pub fn get(&self, key: &str) -> Option<Pixbuf> {
        let mut cache = self.cache.borrow_mut();
        let now = Instant::now();

        // Check if the entry exists and hasn't expired
        if let Some(entry) = cache.get(key)
            && entry.expires_at > now
        {
            // Entry is valid, return a clone of the pixbuf
            // The get() operation already promoted it to most recently used
            return Some(entry.pixbuf.clone());
        }

        // Entry is expired, fall through to return None

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
        let mut cache = self.cache.borrow_mut();
        let mut total_size = self.total_size.borrow_mut();
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
    pub fn insert(&self, key: String, pixbuf: Pixbuf) {
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
            let mut cache = self.cache.borrow_mut();
            let mut total_size = self.total_size.borrow_mut();

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
        let cache_size = self.cache.borrow().cap();

        // Clone implementation creates a new empty cache with the same parameters
        // This is a shallow clone that doesn't copy the actual cache entries
        Self {
            cache: Rc::new(RefCell::new(LruCache::new(cache_size))),
            max_size: self.max_size,
            total_size: Rc::new(RefCell::new(0)),
            ttl: self.ttl,
        }
    }
}

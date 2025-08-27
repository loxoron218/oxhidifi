use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
    time::{Duration, Instant},
};

use crate::ui::grids::album_grid_state::AlbumGridItem;

/// Represents a single entry in the cache with its associated value and expiration information.
///
/// This struct wraps a value of type `T` along with metadata needed for expiration tracking:
/// - `timestamp`: When the entry was created
/// - `ttl`: How long the entry should remain valid
///
/// # Type Parameters
/// * `T` - The type of value being cached
struct CacheEntry<T> {
    /// The cached value
    value: T,
    /// The instant when this entry was created
    timestamp: Instant,
    /// The time-to-live duration for this entry
    ttl: Duration,
}

impl<T> CacheEntry<T> {
    /// Creates a new cache entry with the specified value and TTL.
    ///
    /// # Arguments
    /// * `value` - The value to cache
    /// * `ttl` - The time-to-live duration for this entry
    ///
    /// # Returns
    /// A new `CacheEntry` instance with the current timestamp
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            timestamp: Instant::now(),
            ttl,
        }
    }

    /// Checks if this cache entry has expired.
    ///
    /// An entry is considered expired if the elapsed time since its creation
    /// exceeds its time-to-live duration.
    ///
    /// # Returns
    /// `true` if the entry has expired, `false` otherwise
    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }
}

/// A thread-safe, time-based cache for storing and retrieving values.
///
/// This cache implementation provides:
/// - Generic storage for any type `T` that implements `Clone`
/// - Automatic expiration of entries based on time-to-live (TTL)
/// - Thread safety through `Arc<RwLock<>>`
/// - Automatic cleanup of expired entries during access operations
///
/// # Type Parameters
/// * `T` - The type of values to be cached, must implement `Clone`
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use your_crate::utils::metadata_cache::MetadataCache;
///
/// let cache: MetadataCache<String> = MetadataCache::new(Duration::from_secs(30));
/// cache.insert("key".to_string(), "value".to_string());
/// let value = cache.get("key");
/// ```
pub struct MetadataCache<T> {
    /// Thread-safe storage of cache entries
    entries: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,

    /// Default time-to-live duration for new entries
    default_ttl: Duration,
}

/// Creates a new cache instance with the specified default TTL.
///
/// # Arguments
/// * `default_ttl` - The default time-to-live duration for cache entries
///
/// # Returns
/// A new `MetadataCache` instance
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use your_crate::utils::metadata_cache::MetadataCache;
///
/// let cache = MetadataCache::new(Duration::from_secs(60)); // 60 second TTL
/// ```
impl<T> MetadataCache<T>
where
    T: Clone,
{
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Retrieves a value from the cache if it exists and hasn't expired.
    ///
    /// This method performs automatic cleanup by removing all expired entries
    /// from the cache before attempting to retrieve the requested value.
    ///
    /// # Arguments
    /// * `key` - The key to look up in the cache
    ///
    /// # Returns
    /// `Some(T)` with a clone of the cached value if it exists and is still valid,
    /// `None` if the key doesn't exist or the entry has expired
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use your_crate::utils::metadata_cache::MetadataCache;
    ///
    /// let cache = MetadataCache::new(Duration::from_secs(30));
    /// cache.insert("key".to_string(), "value".to_string());
    ///
    /// if let Some(value) = cache.get("key") {
    ///     println!("Retrieved value: {}", value);
    /// }
    /// ```
    pub fn get(&self, key: &str) -> Option<T> {
        let mut entries = self.entries.write().unwrap();

        // Remove expired entries before attempting to get the value
        entries.retain(|_, entry| !entry.is_expired());
        entries.get(key).map(|entry| entry.value.clone())
    }

    /// Inserts a value into the cache with the default TTL.
    ///
    /// This method stores a value in the cache with the default time-to-live
    /// duration specified when the cache was created. It also performs automatic
    /// cleanup by removing all expired entries before inserting the new value.
    ///
    /// # Arguments
    /// * `key` - The key to associate with the value
    /// * `value` - The value to cache
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use your_crate::utils::metadata_cache::MetadataCache;
    ///
    /// let cache = MetadataCache::new(Duration::from_secs(30));
    /// cache.insert("key".to_string(), "value".to_string());
    /// ```
    pub fn insert(&self, key: String, value: T) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Inserts a value into the cache with a specific TTL.
    ///
    /// This method stores a value in the cache with a custom time-to-live
    /// duration, overriding the cache's default TTL. It also performs automatic
    /// cleanup by removing all expired entries before inserting the new value.
    ///
    /// # Arguments
    /// * `key` - The key to associate with the value
    /// * `value` - The value to cache
    /// * `ttl` - The custom time-to-live duration for this entry
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use your_crate::utils::metadata_cache::MetadataCache;
    ///
    /// let cache = MetadataCache::new(Duration::from_secs(30));
    /// cache.insert_with_ttl("key".to_string(), "value".to_string(), Duration::from_secs(60));
    /// ```
    pub fn insert_with_ttl(&self, key: String, value: T, ttl: Duration) {
        let mut entries = self.entries.write().unwrap();

        // Remove expired entries before inserting the new value
        entries.retain(|_, entry| !entry.is_expired());
        entries.insert(key, CacheEntry::new(value, ttl));
    }
}

/// Cache for album display information used in the main album grid.
///
/// This cache stores `Vec<AlbumGridItem>` with a 30-second TTL, which is appropriate
/// for UI display data that may change relatively frequently.
pub static ALBUM_DISPLAY_CACHE: LazyLock<MetadataCache<Vec<AlbumGridItem>>> =
    LazyLock::new(|| MetadataCache::new(Duration::from_secs(30)));

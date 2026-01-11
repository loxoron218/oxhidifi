//! DR value caching for performance optimization.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::RwLock;

/// Caches DR values per album directory for performance.
///
/// The `AlbumDrCache` provides thread-safe access to cached DR values
/// to avoid repeated file system operations.
#[derive(Debug, Clone, Default)]
pub struct AlbumDrCache {
    /// Internal cache storage.
    cache: Arc<RwLock<HashMap<PathBuf, String>>>,
}

impl AlbumDrCache {
    /// Creates a new DR cache.
    ///
    /// # Returns
    ///
    /// A new `AlbumDrCache` instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets a DR value from the cache.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Album directory path.
    ///
    /// # Returns
    ///
    /// The cached DR value if it exists, `None` otherwise.
    pub fn get<P: AsRef<Path>>(&self, album_path: P) -> Option<String> {
        self.cache.read().get(album_path.as_ref()).cloned()
    }

    /// Inserts a DR value into the cache.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Album directory path.
    /// * `dr_value` - DR value to cache.
    pub fn insert<P: AsRef<Path>>(&self, album_path: P, dr_value: String) {
        self.cache
            .write()
            .insert(album_path.as_ref().to_path_buf(), dr_value);
    }

    /// Removes a DR value from the cache.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Album directory path.
    pub fn remove<P: AsRef<Path>>(&self, album_path: P) {
        self.cache.write().remove(album_path.as_ref());
    }

    /// Clears the entire cache.
    pub fn clear(&self) {
        self.cache.write().clear();
    }

    /// Gets the current cache size.
    ///
    /// # Returns
    ///
    /// The number of entries in the cache.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Checks if the cache is empty.
    ///
    /// # Returns
    ///
    /// `true` if the cache is empty, `false` otherwise.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }
}

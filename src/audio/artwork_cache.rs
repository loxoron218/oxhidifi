//! Artwork caching utilities.
//!
//! This module provides a simple in-memory cache for artwork paths to avoid
//! repeated file system operations and embedded artwork extraction.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::RwLock;

/// In-memory cache for artwork paths.
///
/// Maps album directory paths to their corresponding artwork file paths.
#[derive(Debug, Default)]
pub struct ArtworkCache {
    cache: Arc<RwLock<HashMap<PathBuf, Option<String>>>>,
}

impl ArtworkCache {
    /// Creates a new artwork cache.
    ///
    /// # Returns
    ///
    /// A new `ArtworkCache` instance.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Gets the cached artwork path for an album directory.
    ///
    /// # Arguments
    ///
    /// * `album_dir` - Path to the album directory.
    ///
    /// # Returns
    ///
    /// An `Option<String>` containing the artwork path if cached, or `None` if not found.
    pub fn get<P: AsRef<Path>>(&self, album_dir: P) -> Option<String> {
        let cache = self.cache.read();
        cache
            .get(&album_dir.as_ref().to_path_buf())
            .cloned()
            .flatten()
    }

    /// Sets the cached artwork path for an album directory.
    ///
    /// # Arguments
    ///
    /// * `album_dir` - Path to the album directory.
    /// * `artwork_path` - Optional path to the artwork file.
    pub fn set<P: AsRef<Path>>(&self, album_dir: P, artwork_path: Option<String>) {
        let mut cache = self.cache.write();
        cache.insert(album_dir.as_ref().to_path_buf(), artwork_path);
    }

    /// Clears the entire cache.
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    /// Removes a specific album directory from the cache.
    ///
    /// # Arguments
    ///
    /// * `album_dir` - Path to the album directory to remove.
    pub fn remove<P: AsRef<Path>>(&self, album_dir: P) {
        let mut cache = self.cache.write();
        cache.remove(&album_dir.as_ref().to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::artwork_cache::ArtworkCache;

    #[test]
    fn test_artwork_cache_basic_operations() {
        let cache = ArtworkCache::new();

        // Test get on empty cache
        assert_eq!(cache.get("/non/existent/album"), None);

        // Test set and get
        cache.set("/test/album", Some("/test/album/folder.jpg".to_string()));
        assert_eq!(
            cache.get("/test/album"),
            Some("/test/album/folder.jpg".to_string())
        );

        // Test set with None
        cache.set("/test/album2", None);
        assert_eq!(cache.get("/test/album2"), None);

        // Test remove
        cache.remove("/test/album");
        assert_eq!(cache.get("/test/album"), None);

        // Test clear
        cache.set("/test/album3", Some("/test/album3/cover.jpg".to_string()));
        cache.clear();
        assert_eq!(cache.get("/test/album3"), None);
    }
}

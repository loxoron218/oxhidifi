//! Thread-safe decoded cover art cache.

pub mod dispatch;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use {async_channel::Sender, libadwaita::gdk::MemoryTexture, parking_lot::Mutex};

use crate::ui::texture_pool::dispatch::{ArtworkDecodeRequest, MAX_SIZES_PER_ALBUM};

/// Thread-safe cache for decoded cover art textures.
///
/// Keyed by `(album_id, size)` so that covers decoded at different
/// zoom levels or view sizes are independently cached.  A small fixed
/// pool of background worker threads processes decode requests
/// concurrently, preventing a full-grid re-decode wave from blocking
/// the UI for seconds on large libraries.
///
/// Both the grid view and column view share the same cache instance,
/// and texture lookups without a specific size (e.g. the player panel)
/// return any cached size for the album.
#[derive(Debug)]
pub struct CoverArtCache {
    /// Guarded cache state: decoded textures and per-album size indices.
    inner: Mutex<CoverCacheInner>,
    /// Map of track ID to album ID, so cover lookups by `track_id` can
    /// resolve to the correct album-level cache entry.
    track_to_album: Mutex<HashMap<i64, i64>>,
    /// `(album_id, size)` keys with a decode request queued or in flight,
    /// so rapid zoom toggling never dispatches the same album×size twice.
    in_flight: Mutex<HashSet<(i64, i32)>>,
    /// Channel sender for dispatching decode requests to the worker.
    /// Wrapped in `Mutex<Option<...>>` so the channel can be closed
    /// during shutdown, allowing the worker thread to exit.
    request_tx: Mutex<Option<Sender<ArtworkDecodeRequest>>>,
}

impl CoverArtCache {
    /// Build an empty cache around an optional request sender.
    ///
    /// `new_shared` spawns the decoder pool and passes its sender; tests
    /// pass `None` to construct a cache without any worker threads.
    fn with_sender(request_tx: Option<Sender<ArtworkDecodeRequest>>) -> Self {
        Self {
            inner: Mutex::new(CoverCacheInner {
                textures: HashMap::new(),
                sizes: HashMap::new(),
            }),
            track_to_album: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            request_tx: Mutex::new(request_tx),
        }
    }

    /// Return the cached texture for a given album ID at a specific size.
    pub fn get(&self, album_id: i64, size: i32) -> Option<Arc<MemoryTexture>> {
        self.inner.lock().textures.get(&(album_id, size)).cloned()
    }

    /// Return a cached texture for an album at the most recently stored size.
    pub fn get_any(&self, album_id: i64) -> Option<Arc<MemoryTexture>> {
        let inner = self.inner.lock();
        let size = *inner.sizes.get(&album_id)?.last()?;
        inner.textures.get(&(album_id, size)).cloned()
    }

    /// Check whether any texture is cached for the given album.
    pub fn has_any(&self, album_id: i64) -> bool {
        self.inner.lock().sizes.contains_key(&album_id)
    }

    /// Insert a decoded texture into the cache by album ID and size.
    ///
    /// The size is recorded as the most recent for the album; if the album
    /// already has [`MAX_SIZES_PER_ALBUM`] sizes, the oldest is evicted.
    pub fn insert(&self, album_id: i64, size: i32, texture: MemoryTexture) {
        let mut inner = self.inner.lock();
        drop(inner.textures.insert((album_id, size), Arc::new(texture)));
        let sizes = inner.sizes.entry(album_id).or_default();
        if let Some(pos) = sizes.iter().position(|&s| s == size) {
            _ = sizes.remove(pos);
        }
        sizes.push(size);
        let evicted: Vec<i32> = sizes
            .drain(..sizes.len().saturating_sub(usize::from(MAX_SIZES_PER_ALBUM)))
            .collect();
        for old in evicted {
            drop(inner.textures.remove(&(album_id, old)));
        }
    }

    /// Record the album that a track belongs to, enabling cache lookups
    /// by track ID to resolve to the album-level cache entry.
    pub fn record_track_album(&self, track_id: i64, album_id: i64) {
        _ = self.track_to_album.lock().insert(track_id, album_id);
    }

    /// Look up a cached cover texture by track ID.
    ///
    /// Resolves `track_id → album_id` using the recorded track-to-album
    /// mapping, then returns any cached texture for that album.
    /// Returns `None` if either the mapping or any texture is missing.
    pub fn get_by_track(&self, track_id: i64) -> Option<Arc<MemoryTexture>> {
        let album_id = *self.track_to_album.lock().get(&track_id)?;
        self.get_any(album_id)
    }

    /// Return the cached album ID for a track, if previously recorded.
    pub fn get_album_for_track(&self, track_id: i64) -> Option<i64> {
        self.track_to_album.lock().get(&track_id).copied()
    }
}

/// Inner state of the cover art cache, guarded by a single mutex.
///
/// `textures` and `sizes` live under one lock so that eviction can remove
/// from both without cross-lock ordering or a consistency window.
#[derive(Debug)]
struct CoverCacheInner {
    /// Map of `(album_id, size)` to decoded texture.
    textures: HashMap<(i64, i32), Arc<MemoryTexture>>,
    /// Map of album ID to the sizes currently cached, oldest first.
    sizes: HashMap<i64, Vec<i32>>,
}

#[cfg(test)]
/// Unit tests for the cover art cache's insert, lookup, and eviction behavior.
pub mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::{
        image_decode::raw_to_texture,
        texture_pool::{CoverArtCache, dispatch::tests::mock_decoded_cover},
    };

    fn make_cache() -> CoverArtCache {
        CoverArtCache::with_sender(None)
    }

    #[test]
    fn get_by_track_unmapped_returns_none() {
        let cache = make_cache();
        assert!(cache.get_by_track(1).is_none());
        assert!(cache.get_album_for_track(1).is_none());
    }

    #[test]
    fn record_track_album_round_trip() {
        let cache = make_cache();
        assert!(cache.get_album_for_track(10).is_none());
        cache.record_track_album(10, 100);
        assert_eq!(cache.get_album_for_track(10), Some(100));
    }

    #[test]
    fn insert_get_round_trips_by_size() -> Result<()> {
        let cache = make_cache();
        ensure!(cache.get(1, 180).is_none(), "missing size must return None");
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 240, raw_to_texture(&mock_decoded_cover()));
        ensure!(cache.get(1, 180).is_some(), "inserted size must be cached");
        ensure!(cache.get(1, 240).is_some(), "each size must be cached");
        ensure!(
            cache.get(1, 120).is_none(),
            "a size that was never inserted must return None"
        );
        Ok(())
    }

    #[test]
    fn get_any_returns_most_recent_size() -> Result<()> {
        let cache = make_cache();
        ensure!(cache.get_any(1).is_none(), "empty cache must return None");
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 240, raw_to_texture(&mock_decoded_cover()));
        let texture = cache.get_any(1);
        ensure!(texture.is_some(), "get_any must find a cached size");
        ensure!(
            texture == cache.get(1, 240),
            "get_any must return the most recently inserted size"
        );
        Ok(())
    }

    #[test]
    fn has_any_reflects_cached_sizes() -> Result<()> {
        let cache = make_cache();
        ensure!(!cache.has_any(1), "empty cache must report no sizes");
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        ensure!(cache.has_any(1), "cached album must report a size");
        ensure!(!cache.has_any(2), "other albums must stay uncached");
        Ok(())
    }

    #[test]
    fn eviction_keeps_only_newest_sizes() -> Result<()> {
        let cache = make_cache();
        for size in [120, 150, 180, 210, 240, 32] {
            cache.insert(1, size, raw_to_texture(&mock_decoded_cover()));
        }
        ensure!(
            cache.get(1, 120).is_none(),
            "the oldest size must be evicted past MAX_SIZES_PER_ALBUM"
        );
        ensure!(
            cache.get(1, 150).is_some(),
            "the second-oldest size must survive eviction"
        );
        ensure!(
            cache.get(1, 32).is_some(),
            "the newest size must survive eviction"
        );
        Ok(())
    }

    #[test]
    fn reinserting_same_size_does_not_count_twice() -> Result<()> {
        let cache = make_cache();
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 120, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 180, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 210, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 240, raw_to_texture(&mock_decoded_cover()));
        cache.insert(1, 32, raw_to_texture(&mock_decoded_cover()));
        ensure!(
            cache.get(1, 120).is_some(),
            "a re-inserted size must not count twice and evict an earlier size"
        );
        ensure!(
            cache.get(1, 180).is_some(),
            "the re-inserted size must still be cached"
        );
        ensure!(
            cache.get_any(1) == cache.get(1, 32),
            "the most recent insertion must win get_any"
        );
        Ok(())
    }

    #[test]
    fn get_by_track_resolves_any_cached_size() -> Result<()> {
        let cache = make_cache();
        ensure!(
            cache.get_by_track(10).is_none(),
            "unmapped track must return None"
        );
        cache.record_track_album(10, 100);
        ensure!(
            cache.get_by_track(10).is_none(),
            "a mapped track without a cached cover must return None"
        );
        cache.insert(100, 240, raw_to_texture(&mock_decoded_cover()));
        ensure!(
            cache.get_by_track(10).is_some(),
            "a mapped track must resolve to a cached cover"
        );
        Ok(())
    }
}

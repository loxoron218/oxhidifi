# Storage Interface Contract

## Purpose

The storage layer abstracts all database and settings persistence behind a trait,
allowing the rest of the application to remain oblivious to the storage backend.

## Trait: `Storage`

```rust
/// Interface for all persistent storage operations.
pub trait Storage: Send + Sync + 'static {
    // --- Library ---
    async fn insert_track(&self, track: NewTrack) -> Result<i64>;
    async fn update_track(&self, id: i64, track: TrackUpdate) -> Result<()>;
    async fn delete_track(&self, id: i64) -> Result<()>;
    async fn get_track(&self, id: i64) -> Result<Option<Track>>;
    async fn get_tracks_by_album(&self, album_id: i64) -> Result<Vec<Track>>;
    async fn get_tracks_by_artist(&self, artist_id: i64) -> Result<Vec<Track>>;
    async fn search_tracks(&self, query: &str) -> Result<Vec<Track>>;

    async fn insert_album(&self, album: NewAlbum) -> Result<i64>;
    async fn get_album(&self, id: i64) -> Result<Option<Album>>;
    async fn get_all_albums(&self) -> Result<Vec<Album>>;
    async fn get_albums_by_artist(&self, artist_id: i64) -> Result<Vec<Album>>;

    async fn insert_artist(&self, artist: NewArtist) -> Result<i64>;
    async fn get_artist(&self, id: i64) -> Result<Option<Artist>>;
    async fn get_all_artists(&self) -> Result<Vec<Artist>>;

    async fn list_library_directories(&self) -> Result<Vec<LibraryDirectory>>;
    async fn add_library_directory(&self, path: &Path) -> Result<()>;
    async fn remove_library_directory(&self, id: i64) -> Result<()>;

    // --- Queue ---
    async fn get_queue(&self) -> Result<Vec<QueueEntry>>;
    async fn set_queue(&self, entries: &[NewQueueEntry]) -> Result<()>;
    async fn append_queue(&self, track_id: i64, context: Option<QueueContext>) -> Result<()>;
    async fn remove_queue_entry(&self, id: i64) -> Result<()>;
    async fn reorder_queue(&self, entry_id: i64, new_position: u32) -> Result<()>;
    async fn clear_queue(&self) -> Result<()>;

    // --- Duplicate Detection ---
    async fn find_by_path(&self, path: &Path) -> Result<Option<Track>>;
    async fn find_by_hash(&self, hash: &str) -> Result<Vec<Track>>;
    async fn find_by_metadata_fingerprint(&self, artist: &str, album: &str, title: &str, track: Option<u32>) -> Result<Vec<Track>>;
}

/// Context that describes how a track was added to the queue.
pub enum QueueContext {
    Album(i64),
    Artist(i64),
    Manual,
}
```

## Implementation: `SqliteStorage`

- Backed by a single SQLite database at `$XDG_DATA_HOME/oxhidifi/library.db`
- Uses `sqlx::SqlitePool` for connection pooling (pool size = 1 for simplicity — SQLite is single-writer)
- Compile-time checked queries via `sqlx::query!()` / `sqlx::query_as!()`
- Migrations managed via `sqlx::migrate!()` macro or manual schema versioning

## Implementation: `SettingsStore`

- Reads/writes JSON at `$XDG_CONFIG_HOME/oxhidifi/settings.json`
- Uses `serde_json` for serialization
- Loaded at startup, written on mutation
- Not behind the `Storage` trait — accessed directly by the settings UI

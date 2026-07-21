//! `SQLite` database implementation using `sqlx` for library catalog persistence.

use std::{collections::HashMap, fs::write, path::Path};

use {
    parking_lot::RwLock,
    serde_json::to_string_pretty,
    sqlx::{
        FromRow, QueryBuilder, SqlitePool, query, query_as,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    },
    tokio::task::spawn_blocking,
    tracing::warn,
};

use crate::{
    playback::output::OutputMode,
    storage::{
        Album, Artist,
        FieldUpdate::{Set, SetNull, Skip},
        FormatInfo, LibraryDirectory, NewAlbum, NewArtist, NewQueueEntry, NewTrack,
        QueueContext::{self, Album as QueueAlbum, Artist as QueueArtist, Manual},
        QueueEntry, Storage,
        StorageError::{self, Database, InvalidPath},
        StorageResult, Track, TrackUpdate,
        migrations::run,
        settings::{ActiveTab, SettingsStore, ViewMode},
    },
};

/// Subquery fragment for album count and duration columns.
macro_rules! album_meta_cols {
    () => {
        "(SELECT COUNT(*) FROM tracks WHERE album_id = al.id) AS track_count, (SELECT \
         COALESCE(SUM(duration), 0.0) FROM tracks WHERE album_id = al.id) AS total_duration, \
         al.format_summary, al.lossless, al.format, al.bit_depth, al.sample_rate FROM albums al"
    };
}

/// Apply a `FieldUpdate` to a column in the tracks table.
macro_rules! apply_field {
    ($track:expr, $field:ident, $pool:expr, $id:expr) => {
        match &$track.$field {
            Set(v) => {
                query(concat!(
                    "UPDATE tracks SET ",
                    stringify!($field),
                    " = ? WHERE id = ?"
                ))
                .bind(v)
                .bind($id)
                .execute(&$pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
            }
            SetNull => {
                query(concat!(
                    "UPDATE tracks SET ",
                    stringify!($field),
                    " = NULL WHERE id = ?"
                ))
                .bind($id)
                .execute(&$pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
            }
            Skip => {}
        }
    };
}

impl From<FormatInfoRow> for FormatInfo {
    fn from(row: FormatInfoRow) -> Self {
        raw_info_to_format_info(
            row.formats,
            row.sample_rates.as_deref(),
            row.bit_depths.as_deref(),
            row.channels.as_deref(),
        )
    }
}

/// Raw row from the `GROUP_CONCAT` format info query.
#[derive(Debug, Clone, FromRow)]
struct FormatInfoRow {
    /// Album identifier.
    album_id: i64,
    /// Comma-separated distinct format/codec names.
    formats: Option<String>,
    /// Comma-separated distinct sample rates.
    sample_rates: Option<String>,
    /// Comma-separated distinct bit depths.
    bit_depths: Option<String>,
    /// Comma-separated distinct channel counts.
    channels: Option<String>,
}

/// SQLite-backed storage implementation.
pub struct SqliteStorage {
    /// `SQLite` connection pool.
    pool: SqlitePool,
    /// User settings store.
    settings: RwLock<SettingsStore>,
}

impl SqliteStorage {
    /// Inserts a new track row into the database and returns its ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the insert query fails.
    async fn insert_track_row(&self, track: &NewTrack) -> StorageResult<i64> {
        let row_id: (i64,) = query_as(
            "INSERT INTO tracks (title, number, disc_number, duration, file_path, content_hash, \
             format, sample_rate, bit_depth, channels, codec, lossless, bitrate, album_id, \
             artist_id, file_size, last_modified) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
             ?, ?, ?, ?) RETURNING id",
        )
        .bind(&track.title)
        .bind(track.track_number)
        .bind(track.disc_number)
        .bind(track.duration)
        .bind(&track.audio.file_path)
        .bind(&track.audio.content_hash)
        .bind(&track.audio.format)
        .bind(track.audio.sample_rate)
        .bind(track.audio.bit_depth)
        .bind(track.audio.channels)
        .bind(&track.audio.codec)
        .bind(track.audio.lossless)
        .bind(track.audio.bitrate)
        .bind(track.audio.album_id)
        .bind(track.audio.artist_id)
        .bind(track.audio.file_size)
        .bind(&track.audio.last_modified)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Database(format!("Insert track failed: {e}")))?;

        Ok(row_id.0)
    }

    /// Create a new `SqliteStorage` with a connection pool to the given database path.
    ///
    /// Runs migrations on connect.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created or migrations fail.
    pub async fn connect(database_path: &Path) -> StorageResult<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .map_err(|e| Database(format!("Failed to connect: {e}")))?;

        run(&pool).await?;

        let settings = SettingsStore::load_async()
            .await
            .map_err(|e| Database(format!("Failed to load settings: {e}")))?;

        Ok(Self {
            pool,
            settings: RwLock::new(settings),
        })
    }

    /// Get the current view mode.
    pub fn get_view_mode(&self) -> ViewMode {
        self.settings.read().get().view_mode
    }

    /// Set the view mode in memory and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings cannot be saved.
    pub async fn set_view_mode(&self, mode: ViewMode) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| s.view_mode = mode);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save view mode: {e}")))?;
        Ok(())
    }

    /// Get whether gapless playback is enabled.
    pub fn get_gapless_enabled(&self) -> bool {
        self.settings.read().get_gapless_enabled()
    }

    /// Set whether gapless playback is enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_gapless_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.gapless_enabled = enabled);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save gapless setting: {e}")))?;
        Ok(())
    }

    /// Get the preferred audio device name.
    pub fn get_audio_device(&self) -> Option<String> {
        self.settings.read().get_audio_device().map(String::from)
    }

    /// Set the preferred audio device name.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_audio_device(&self, device: Option<String>) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.audio_device = device);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save audio device: {e}")))?;
        Ok(())
    }

    /// Get the active tab preference.
    pub fn get_active_tab(&self) -> ActiveTab {
        self.settings.read().get_active_tab()
    }

    /// Set the active tab preference.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_active_tab(&self, tab: ActiveTab) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| s.active_tab = tab);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save active tab: {e}")))?;
        Ok(())
    }

    /// Get the volume level from settings.
    pub fn get_settings_volume(&self) -> f64 {
        self.settings.read().get_volume()
    }

    /// Set the volume level in memory and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings cannot be saved.
    pub async fn set_volume(&self, volume: f64) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| s.volume = volume);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save volume: {e}")))?;
        Ok(())
    }

    /// Get the output mode from settings.
    pub fn get_output_mode(&self) -> OutputMode {
        self.settings.read().get_output_mode()
    }

    /// Set the output mode in memory and persist to disk asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings cannot be saved.
    pub async fn set_output_mode(&self, mode: OutputMode) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.output_mode = mode);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save output mode: {e}")))?;
        Ok(())
    }

    /// Serialize settings to JSON and persist to disk via `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Database` if JSON serialization or file writing fails.
    async fn save_settings_async(&self) -> Result<(), StorageError> {
        let (json, path) = {
            let settings = self.settings.read();
            let json = to_string_pretty(settings.get())
                .map_err(|e| Database(format!("Failed to serialize settings: {e}")))?;
            (json, settings.path().to_path_buf())
        };
        let path_for_error = path.clone();
        spawn_blocking(move || write(&path, &json))
            .await
            .map_err(|e| Database(format!("Failed to spawn blocking write: {e}")))?
            .map_err(|e| {
                Database(format!(
                    "Failed to write settings to {}: {e}",
                    path_for_error.display()
                ))
            })?;
        Ok(())
    }
}

impl Storage for SqliteStorage {
    async fn insert_track(&self, track: NewTrack) -> StorageResult<i64> {
        self.insert_track_row(&track).await
    }

    async fn update_track(&self, id: i64, track: TrackUpdate) -> StorageResult<()> {
        if let Some(title) = track.title {
            query("UPDATE tracks SET title = ? WHERE id = ?")
                .bind(&title)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
        }
        apply_field!(track, track_number, self.pool, id);
        apply_field!(track, disc_number, self.pool, id);
        if let Some(duration) = track.duration {
            query("UPDATE tracks SET duration = ? WHERE id = ?")
                .bind(duration)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| Database(format!("Update track failed: {e}")))?;
        }
        apply_field!(track, content_hash, self.pool, id);
        apply_field!(track, album_id, self.pool, id);
        apply_field!(track, artist_id, self.pool, id);
        Ok(())
    }

    async fn delete_track(&self, id: i64) -> StorageResult<()> {
        query("DELETE FROM tracks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Delete track failed: {e}")))?;
        Ok(())
    }

    async fn get_track(&self, id: i64) -> StorageResult<Option<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Database(format!("Get track failed: {e}")))
    }

    async fn get_tracks_by_album(&self, album_id: i64) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE album_id = ? ORDER BY number")
            .bind(album_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by album failed: {e}")))
    }

    async fn get_tracks_by_artist(&self, artist_id: i64) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE artist_id = ?")
            .bind(artist_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by artist failed: {e}")))
    }

    async fn search_tracks(&self, query: &str) -> StorageResult<Vec<Track>> {
        let pattern = format!("%{query}%");
        query_as::<_, Track>("SELECT * FROM tracks WHERE title LIKE ? OR file_path LIKE ?")
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Search tracks failed: {e}")))
    }

    async fn insert_album(&self, album: NewAlbum) -> StorageResult<i64> {
        let row_id: (i64,) = query_as(
            "INSERT INTO albums (title, artist_id, year, genre, artwork_path, format_summary, \
             lossless, format, bit_depth, sample_rate) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             RETURNING id",
        )
        .bind(&album.title)
        .bind(album.artist_id)
        .bind(album.year)
        .bind(&album.genre)
        .bind(&album.artwork_path)
        .bind(&album.format_summary)
        .bind(album.lossless)
        .bind(&album.format)
        .bind(album.bit_depth)
        .bind(album.sample_rate)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Database(format!("Insert album failed: {e}")))?;

        Ok(row_id.0)
    }

    async fn get_album(&self, id: i64) -> StorageResult<Option<Album>> {
        query_as::<_, Album>(concat!(
            "SELECT al.id, al.title, al.artist_id, al.year, al.genre, al.artwork_path, ",
            album_meta_cols!(),
            " WHERE al.id = ?",
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Database(format!("Get album failed: {e}")))
    }

    async fn get_all_albums(&self) -> StorageResult<Vec<Album>> {
        query_as::<_, Album>(concat!(
            "SELECT al.id, al.title, al.artist_id, al.year, al.genre, al.artwork_path, ",
            album_meta_cols!(),
            " ORDER BY al.title",
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Get all albums failed: {e}")))
    }

    async fn get_album_format_info(&self, album_id: i64) -> StorageResult<FormatInfo> {
        #[derive(Debug, Clone, FromRow)]
        struct RawInfo {
            formats: Option<String>,
            sample_rates: Option<String>,
            bit_depths: Option<String>,
            channels: Option<String>,
        }

        let row: Option<RawInfo> = query_as(
            "SELECT GROUP_CONCAT(DISTINCT UPPER(codec)) AS formats, GROUP_CONCAT(DISTINCT \
             sample_rate) AS sample_rates, GROUP_CONCAT(DISTINCT bit_depth) AS bit_depths, \
             GROUP_CONCAT(DISTINCT channels) AS channels FROM tracks WHERE album_id = ?",
        )
        .bind(album_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Database(format!("Get album format info failed: {e}")))?;

        Ok(row.map_or_else(FormatInfo::default, |r| {
            raw_info_to_format_info(
                r.formats,
                r.sample_rates.as_deref(),
                r.bit_depths.as_deref(),
                r.channels.as_deref(),
            )
        }))
    }

    async fn get_albums_format_info(
        &self,
        album_ids: &[i64],
    ) -> StorageResult<HashMap<i64, FormatInfo>> {
        if album_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut builder = QueryBuilder::new(
            "SELECT album_id, GROUP_CONCAT(DISTINCT UPPER(codec)) AS formats, \
             GROUP_CONCAT(DISTINCT sample_rate) AS sample_rates, GROUP_CONCAT(DISTINCT bit_depth) \
             AS bit_depths, GROUP_CONCAT(DISTINCT channels) AS channels FROM tracks WHERE \
             album_id IN (",
        );

        let mut separated = builder.separated(", ");
        for id in album_ids {
            separated.push_bind(id);
        }
        builder.push(") GROUP BY album_id");

        let rows: Vec<FormatInfoRow> = builder
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get albums format info failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| (r.album_id, FormatInfo::from(r)))
            .collect())
    }

    async fn get_albums_by_artist(&self, artist_id: i64) -> StorageResult<Vec<Album>> {
        query_as::<_, Album>(concat!(
            "SELECT al.id, al.title, al.artist_id, al.year, al.genre, al.artwork_path, ",
            album_meta_cols!(),
            " WHERE al.artist_id = ? ORDER BY al.year",
        ))
        .bind(artist_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Get albums by artist failed: {e}")))
    }

    async fn insert_artist(&self, artist: NewArtist) -> StorageResult<i64> {
        let row_id: (i64,) = query_as("INSERT INTO artists (name) VALUES (?) RETURNING id")
            .bind(&artist.name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Database(format!("Insert artist failed: {e}")))?;

        Ok(row_id.0)
    }

    async fn get_artist(&self, id: i64) -> StorageResult<Option<Artist>> {
        query_as::<_, Artist>(
            "SELECT ar.id, ar.name, (SELECT COUNT(*) FROM albums WHERE artist_id = ar.id) AS \
             album_count FROM artists ar WHERE ar.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Database(format!("Get artist failed: {e}")))
    }

    async fn get_all_artists(&self) -> StorageResult<Vec<Artist>> {
        query_as::<_, Artist>(
            "SELECT ar.id, ar.name, (SELECT COUNT(*) FROM albums WHERE artist_id = ar.id) AS \
             album_count FROM artists ar ORDER BY ar.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Get all artists failed: {e}")))
    }

    async fn list_library_directories(&self) -> StorageResult<Vec<LibraryDirectory>> {
        query_as::<_, LibraryDirectory>("SELECT * FROM library_directories ORDER BY path")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("List directories failed: {e}")))
    }

    async fn add_library_directory(&self, path: &Path) -> StorageResult<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| InvalidPath(path.display().to_string()))?;

        query("INSERT OR IGNORE INTO library_directories (path) VALUES (?)")
            .bind(path_str)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Add directory failed: {e}")))?;

        Ok(())
    }

    async fn remove_library_directory(&self, id: i64) -> StorageResult<()> {
        query("DELETE FROM library_directories WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Remove directory failed: {e}")))?;

        Ok(())
    }

    async fn get_queue(&self) -> StorageResult<Vec<QueueEntry>> {
        query_as::<_, QueueEntry>("SELECT * FROM playback_queue ORDER BY position")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get queue failed: {e}")))
    }

    async fn set_queue(&self, entries: &[NewQueueEntry]) -> StorageResult<()> {
        query("DELETE FROM playback_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Clear queue failed: {e}")))?;

        for entry in entries {
            query(
                "INSERT INTO playback_queue (track_id, position, context_type, context_id) VALUES \
                 (?, ?, ?, ?)",
            )
            .bind(entry.track_id)
            .bind(entry.position)
            .bind(&entry.context_type)
            .bind(entry.context_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Set queue entry failed: {e}")))?;
        }

        Ok(())
    }

    async fn append_queue(
        &self,
        track_id: i64,
        context: Option<QueueContext>,
    ) -> StorageResult<()> {
        let max_pos: Option<(i32,)> =
            query_as("SELECT COALESCE(MAX(position), -1) FROM playback_queue")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| Database(format!("Queue max failed: {e}")))?;

        let next_pos = max_pos.map_or(0, |(p,)| p + 1);

        let (context_type, context_id) = match context {
            Some(QueueAlbum(id)) => (Some("album".to_string()), Some(id)),
            Some(QueueArtist(id)) => (Some("artist".to_string()), Some(id)),
            Some(Manual) | None => (None, None),
        };

        query(
            "INSERT INTO playback_queue (track_id, position, context_type, context_id) VALUES (?, \
             ?, ?, ?)",
        )
        .bind(track_id)
        .bind(next_pos)
        .bind(context_type)
        .bind(context_id)
        .execute(&self.pool)
        .await
        .map_err(|e| Database(format!("Append queue failed: {e}")))?;

        Ok(())
    }

    async fn remove_queue_entry(&self, id: i64) -> StorageResult<()> {
        query("DELETE FROM playback_queue WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Remove queue entry failed: {e}")))?;

        Ok(())
    }

    async fn reorder_queue(&self, entry_id: i64, new_position: u32) -> StorageResult<()> {
        query("UPDATE playback_queue SET position = ? WHERE id = ?")
            .bind(new_position.cast_signed())
            .bind(entry_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Reorder queue failed: {e}")))?;

        Ok(())
    }

    async fn clear_queue(&self) -> StorageResult<()> {
        query("DELETE FROM playback_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| Database(format!("Clear queue failed: {e}")))?;

        Ok(())
    }

    async fn find_by_path(&self, path: &Path) -> StorageResult<Option<Track>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| InvalidPath(path.display().to_string()))?;

        query_as::<_, Track>("SELECT * FROM tracks WHERE file_path = ?")
            .bind(path_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Database(format!("Find by path failed: {e}")))
    }

    async fn find_by_hash(&self, hash: &str) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>("SELECT * FROM tracks WHERE content_hash = ?")
            .bind(hash)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Find by hash failed: {e}")))
    }

    async fn find_by_metadata_fingerprint(
        &self,
        artist: &str,
        album: &str,
        title: &str,
        track: Option<u32>,
    ) -> StorageResult<Vec<Track>> {
        query_as::<_, Track>(
            "SELECT t.* FROM tracks t JOIN albums a ON t.album_id = a.id JOIN artists ar ON \
             t.artist_id = ar.id WHERE ar.name = ? AND a.title = ? AND t.title = ? AND (? IS NULL \
             OR t.number = ?)",
        )
        .bind(artist)
        .bind(album)
        .bind(title)
        .bind(track.map(u32::cast_signed))
        .bind(track.map(u32::cast_signed))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Find by fingerprint failed: {e}")))
    }

    async fn insert_tracks_batch(&self, tracks: Vec<NewTrack>) -> StorageResult<Vec<i64>> {
        let mut ids = Vec::with_capacity(tracks.len());
        for track in &tracks {
            ids.push(self.insert_track_row(track).await?);
        }
        Ok(ids)
    }

    async fn find_by_paths_batch(&self, paths: &[&Path]) -> StorageResult<Vec<Option<Track>>> {
        let mut results = Vec::with_capacity(paths.len());
        for path in paths {
            let path_str = path
                .to_str()
                .ok_or_else(|| InvalidPath(path.display().to_string()))?;

            let track = query_as::<_, Track>("SELECT * FROM tracks WHERE file_path = ?")
                .bind(path_str)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| Database(format!("Find by path failed: {e}")))?;
            results.push(track);
        }
        Ok(results)
    }

    async fn find_by_hashes_batch(&self, hashes: &[&str]) -> StorageResult<Vec<Vec<Track>>> {
        let mut results = Vec::with_capacity(hashes.len());
        for hash in hashes {
            let tracks = query_as::<_, Track>("SELECT * FROM tracks WHERE content_hash = ?")
                .bind(hash)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Database(format!("Find by hash failed: {e}")))?;
            results.push(tracks);
        }
        Ok(results)
    }

    async fn get_tracks_by_albums(&self, album_ids: &[i64]) -> StorageResult<Vec<Track>> {
        if album_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::new("SELECT * FROM tracks WHERE album_id IN (");
        let mut separated = builder.separated(", ");
        for id in album_ids {
            separated.push_bind(id);
        }
        builder.push(") ORDER BY album_id, number");
        builder
            .build_query_as::<Track>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by albums failed: {e}")))
    }

    async fn get_tracks_by_ids(&self, ids: &[i64]) -> StorageResult<Vec<Track>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut builder = QueryBuilder::new("SELECT * FROM tracks WHERE id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        builder.push(")");
        builder
            .build_query_as::<Track>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get tracks by ids failed: {e}")))
    }
}

/// Parse a comma-separated string of integers, logging parse failures.
fn parse_int_list(s: &str) -> Vec<i32> {
    s.split(',')
        .filter_map(|v| {
            let trimmed = v.trim();
            match trimmed.parse::<i32>() {
                Ok(n) => Some(n),
                Err(e) => {
                    warn!(
                        error = %e,
                        value = trimmed,
                        "Skipping unparseable integer in format info",
                    );
                    None
                }
            }
        })
        .collect()
}

/// Parse comma-separated format info strings into a `FormatInfo`.
fn raw_info_to_format_info(
    formats: Option<String>,
    sample_rates: Option<&str>,
    bit_depths: Option<&str>,
    channels: Option<&str>,
) -> FormatInfo {
    FormatInfo {
        formats: formats.map_or_else(Vec::new, |s| {
            s.split(',').map(str::trim).map(str::to_string).collect()
        }),
        sample_rates: sample_rates.map_or_else(Vec::new, parse_int_list),
        bit_depths: bit_depths.map_or_else(Vec::new, parse_int_list),
        channels: channels.map_or_else(Vec::new, parse_int_list),
    }
}

//! `SQLite` database implementation using `sqlx` for library catalog persistence.

use std::{
    collections::HashMap,
    fs::write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
};

use {
    parking_lot::{Mutex, RwLock},
    serde_json::to_string_pretty,
    sqlx::{
        FromRow, QueryBuilder, SqlitePool, query, query_as,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    },
    tokio::{
        spawn,
        task::spawn_blocking,
        time::{Duration, Instant, sleep},
    },
    tracing::warn,
};

use crate::{
    app::dirs_config_home,
    playback::devices::OutputMode,
    storage::{
        Storage,
        StorageError::{self, Database, InvalidPath},
        StorageResult,
        formats::FormatInfo,
        migrations::run,
        records::{
            Album, Artist,
            FieldUpdate::{Set, SetNull, Skip},
            LibraryDirectory, NewAlbum, NewArtist, NewQueueEntry, NewTrack,
            QueueContext::{self, Album as QueueAlbum, Artist as QueueArtist, Manual},
            QueueEntry, Track, TrackUpdate,
        },
        settings::{ActiveTab, SettingsStore, ViewMode},
        sort_rules::{AlbumSortItem, ArtistSortItem},
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
    /// Monotonically increasing sequence number for settings save requests.
    /// Used by the debounce logic to determine which request is the latest.
    last_save_seq: AtomicU64,
    /// Tracks the `(sequence, Instant)` of the most recent settings-save
    /// request so that concurrent callers can debounce correctly without
    /// losing the final write.
    last_save_req: Mutex<(u64, Instant)>,
    /// Set while a settings write is in flight, guaranteeing that at most
    /// one `spawn_blocking` disk write runs at a time.
    write_in_flight: AtomicBool,
    /// Set when a save request arrived while a write was in flight; the
    /// completing writer re-runs so the newest in-memory state always lands.
    write_pending: AtomicBool,
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
    /// Runs migrations on connect and loads settings from the default XDG
    /// config path.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created or migrations fail.
    pub async fn connect(database_path: &Path) -> StorageResult<Self> {
        let settings_path = dirs_config_home()
            .map_err(|e| Database(format!("Failed to resolve config dir: {e}")))?
            .join("oxhidifi")
            .join("settings.json");
        Self::connect_with_settings_path(database_path, &settings_path).await
    }

    /// Create a new `SqliteStorage` with a connection pool to the given database
    /// path and a custom settings file path.
    ///
    /// Runs migrations on connect. The settings path seam lets tests target a
    /// temporary directory instead of the user's real config.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created or migrations fail.
    pub async fn connect_with_settings_path(
        database_path: &Path,
        settings_path: &Path,
    ) -> StorageResult<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .map_err(|e| Database(format!("Failed to connect: {e}")))?;

        run(&pool).await?;

        let settings = SettingsStore::load_from_path(settings_path)
            .await
            .map_err(|e| Database(format!("Failed to load settings: {e}")))?;

        Ok(Self {
            pool,
            settings: RwLock::new(settings),
            last_save_seq: AtomicU64::new(0),
            last_save_req: Mutex::new((0, Instant::now())),
            write_in_flight: AtomicBool::new(false),
            write_pending: AtomicBool::new(false),
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

    /// Get whether album labels are shown under cover art.
    pub fn get_show_album_labels(&self) -> bool {
        self.settings.read().get_show_album_labels()
    }

    /// Set whether album labels are shown under cover art.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_show_album_labels(&self, enabled: bool) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.show_album_labels = enabled);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save album labels setting: {e}")))?;
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

    /// Get the albums sort configuration.
    pub fn get_albums_sort(&self) -> Vec<AlbumSortItem> {
        self.settings.read().get().albums_sort.clone()
    }

    /// Set the albums sort configuration in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_albums_sort_memory(&self, items: Vec<AlbumSortItem>) {
        self.settings
            .write()
            .update_memory(|s| s.albums_sort = items);
    }

    /// Get the artists sort configuration.
    pub fn get_artists_sort(&self) -> Vec<ArtistSortItem> {
        self.settings.read().get().artists_sort.clone()
    }

    /// Set the artists sort configuration in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_artists_sort_memory(&self, items: Vec<ArtistSortItem>) {
        self.settings
            .write()
            .update_memory(|s| s.artists_sort = items);
    }

    /// Get the grid zoom level.
    pub fn get_grid_zoom_level(&self) -> u8 {
        self.settings.read().get().grid_zoom_level
    }

    /// Set the grid zoom level in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_grid_zoom_level_memory(&self, level: u8) {
        self.settings
            .write()
            .update_memory(|s| s.grid_zoom_level = level);
    }

    /// Get the list zoom level.
    pub fn get_list_zoom_level(&self) -> u8 {
        self.settings.read().get().list_zoom_level
    }

    /// Set the list zoom level in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_list_zoom_level_memory(&self, level: u8) {
        self.settings
            .write()
            .update_memory(|s| s.list_zoom_level = level);
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

    /// Set the volume level in memory only.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_volume_memory(&self, volume: f64) {
        self.settings.write().update_memory(|s| s.volume = volume);
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

    /// Set the output mode in memory only.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_output_mode_memory(&self, mode: OutputMode) {
        self.settings
            .write()
            .update_memory(|s| s.output_mode = mode);
    }

    /// Wait until 100 ms have elapsed since the most recent save request,
    /// or bail early if a newer request supersedes `my_seq`.
    /// Returns `true` when this caller should proceed with the write.
    async fn debounce_save(&self, my_seq: u64) -> bool {
        loop {
            let (latest_seq, latest_time) = *self.last_save_req.lock();
            let elapsed = latest_time.elapsed();
            match Duration::from_millis(100).checked_sub(elapsed) {
                Some(remaining) => sleep(remaining).await,
                None => return latest_seq == my_seq,
            }
        }
    }

    /// Debounce a settings save and write it to disk, draining any request
    /// that arrives while the write is in flight.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Database` if serialization or the file write fails.
    async fn save_settings_async(&self) -> Result<(), StorageError> {
        drain_settings_saves(self).await
    }

    /// Serialize the current in-memory settings and write them to disk.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Database` if serialization or the file write fails.
    async fn write_settings_to_disk(&self) -> Result<(), StorageError> {
        let json = to_string_pretty(self.settings.read().get())
            .map_err(|e| Database(format!("Failed to serialize settings: {e}")))?;
        let path = self.settings.read().path().to_path_buf();
        let path_for_error = path.clone();
        spawn_blocking(move || write(&path, &json))
            .await
            .map_err(|e| Database(e.to_string()))?
            .map_err(|e| {
                Database(format!(
                    "Failed to write settings {}: {e}",
                    path_for_error.display()
                ))
            })
    }

    /// Schedule a debounced settings write to disk in the background.
    ///
    /// In-memory settings are committed synchronously by the `*_memory`
    /// setters; this method coalesces rapid changes into a single disk
    /// write.  Errors are logged on the background task.
    pub fn save_settings(self: &Arc<Self>) {
        let me = Arc::clone(self);
        spawn(async move {
            Self::persist_settings(&me).await;
        });
    }

    /// Write settings to disk with a debounce, logging any failure.
    async fn persist_settings(storage: &Self) {
        if let Err(e) = storage.save_settings_async().await {
            warn!(error = %e, "Failed to persist settings");
        }
    }

    /// Get the last playback session data from settings.
    pub fn get_last_session(&self) -> (Vec<i64>, Option<usize>, Option<i64>, f64, f64) {
        self.settings.read().get_last_session()
    }

    /// Persist the current playback session to settings synchronously.
    ///
    /// Used on window close, where the write must complete before the
    /// process exits. Bypasses the debounced async save, which may be
    /// dropped once the main loop stops before the debounce window
    /// elapses.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings file cannot be written.
    pub fn set_last_session(
        &self,
        queue: Vec<i64>,
        queue_index: Option<usize>,
        track_id: Option<i64>,
        position: f64,
        duration: f64,
    ) -> Result<(), StorageError> {
        self.settings.write().update_memory(|s| {
            s.last_queue = queue;
            s.last_queue_index = queue_index;
            s.last_track_id = track_id;
            s.last_position = position;
            s.last_duration = duration;
        });
        self.settings
            .read()
            .save_sync()
            .map_err(|e| Database(format!("Failed to save session: {e}")))
    }

    /// Load file paths for a list of track IDs.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn get_track_paths(&self, ids: &[i64]) -> StorageResult<HashMap<i64, PathBuf>> {
        let tracks = self.get_tracks_by_ids(ids).await?;
        Ok(tracks
            .into_iter()
            .map(|t| (t.id, PathBuf::from(t.audio.file_path)))
            .collect())
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

/// Keep writing settings to disk until no further request is pending.
///
/// Debounces by waiting for a 100 ms quiet period after the most recent
/// request: if a newer request arrives during the wait, this pass bails and
/// lets the newer request drive the write, so the *last* change in a burst
/// always persists.
///
/// Writes are serialized through [`SqliteStorage::write_in_flight`]: at most
/// one disk write runs at a time. A request that arrives mid-write marks
/// [`SqliteStorage::write_pending`] and the completing pass drains it, so the
/// newest in-memory state always lands without concurrent writers racing.
/// Free function so the loop body stays below the clippy nesting threshold
/// (a method would add an extra level).
///
/// # Errors
///
/// Returns `StorageError::Database` if serialization or the file write fails.
async fn drain_settings_saves(storage: &SqliteStorage) -> Result<(), StorageError> {
    loop {
        let my_seq = storage.last_save_seq.fetch_add(1, Relaxed);
        *storage.last_save_req.lock() = (my_seq, Instant::now());

        if !storage.debounce_save(my_seq).await {
            return Ok(());
        }

        if storage.write_in_flight.swap(true, Relaxed) {
            storage.write_pending.store(true, Relaxed);
            return Ok(());
        }

        let result = storage.write_settings_to_disk().await;
        storage.write_in_flight.store(false, Relaxed);
        if storage.write_pending.swap(false, Relaxed) {
            continue;
        }
        return result;
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

#[cfg(test)]
mod tests {
    use std::{
        fs::{read_to_string, remove_dir_all, write},
        path::Path,
        sync::{Arc, atomic::Ordering::Relaxed},
    };

    use {
        anyhow::{Context, Result, bail, ensure},
        serde_json::from_str,
        tempfile::{TempDir, tempdir},
        tokio::{
            test,
            time::{Duration, sleep, timeout},
        },
    };

    use crate::{
        playback::devices::OutputMode::BitPerfect,
        storage::{
            StorageError::Database,
            database::SqliteStorage,
            sort_rules::{
                AlbumSortCriteria::{BitDepth, Title},
                AlbumSortItem,
                ArtistSortCriteria::Name,
                ArtistSortItem,
                SortOrder::{Ascending, Descending},
            },
            user_settings::UserSettings,
        },
    };

    async fn storage_in(dir: &TempDir) -> Result<SqliteStorage> {
        let db = dir.path().join("library.db");
        let settings = dir.path().join("settings.json");
        SqliteStorage::connect_with_settings_path(&db, &settings)
            .await
            .context("storage should connect")
    }

    async fn wait_for_quiet_write(path: &Path, storage: &SqliteStorage) {
        while !path.exists()
            || storage.write_in_flight.load(Relaxed)
            || storage.write_pending.load(Relaxed)
        {
            sleep(Duration::from_millis(10)).await;
        }
    }

    async fn await_write_drain(storage: &SqliteStorage) -> Result<()> {
        let path = storage.settings.read().path().to_path_buf();
        timeout(Duration::from_secs(2), wait_for_quiet_write(&path, storage))
            .await
            .context("settings write should drain")
    }

    fn burst_zoom_saves(storage: &Arc<SqliteStorage>) {
        for level in 0..10 {
            storage.set_grid_zoom_level_memory(level);
            storage.save_settings();
        }
    }

    fn read_grid_zoom(dir: &TempDir) -> Result<u8> {
        let content = read_to_string(dir.path().join("settings.json"))?;
        let restored: UserSettings = from_str(&content)?;
        Ok(restored.grid_zoom_level)
    }

    #[test]
    async fn save_settings_persists_latest_value() -> Result<()> {
        let dir = tempdir()?;
        let storage = Arc::new(storage_in(&dir).await?);

        storage.set_grid_zoom_level_memory(3);
        storage.save_settings();

        await_write_drain(&storage).await?;
        ensure!(
            read_grid_zoom(&dir)? == 3,
            "latest in-memory value must persist"
        );
        Ok(())
    }

    #[test]
    async fn burst_saves_coalesce_to_latest_value() -> Result<()> {
        let dir = tempdir()?;
        let storage = Arc::new(storage_in(&dir).await?);

        burst_zoom_saves(&storage);

        await_write_drain(&storage).await?;
        ensure!(read_grid_zoom(&dir)? == 9, "newest burst value must win");
        Ok(())
    }

    #[test]
    async fn save_error_includes_settings_path() -> Result<()> {
        let dir = tempdir()?;
        let settings_dir = dir.path().join("settings_dir");
        let settings = settings_dir.join("settings.json");
        let db = dir.path().join("library.db");
        let storage = SqliteStorage::connect_with_settings_path(&db, &settings)
            .await
            .context("storage should connect")?;

        remove_dir_all(&settings_dir)?;
        write(&settings_dir, b"x")?;

        let message = match storage.save_settings_async().await {
            Err(Database(m)) => m,
            other => bail!("expected Database error, got {other:?}"),
        };
        ensure!(message.contains("settings.json"), "message was: {message}");
        ensure!(
            message.contains("Failed to write settings"),
            "message was: {message}"
        );
        Ok(())
    }

    #[test]
    async fn albums_sort_memory_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        let items = vec![
            AlbumSortItem {
                criteria: Title,
                order: Descending,
            },
            AlbumSortItem {
                criteria: BitDepth,
                order: Ascending,
            },
        ];
        storage.set_albums_sort_memory(items.clone());
        ensure!(
            storage.get_albums_sort() == items,
            "albums sort must round-trip through the memory setters"
        );
        Ok(())
    }

    #[test]
    async fn artists_sort_memory_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        let items = vec![ArtistSortItem {
            criteria: Name,
            order: Descending,
        }];
        storage.set_artists_sort_memory(items.clone());
        ensure!(
            storage.get_artists_sort() == items,
            "artists sort must round-trip through the memory setters"
        );
        Ok(())
    }

    #[test]
    async fn zoom_levels_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_grid_zoom_level_memory(3);
        storage.set_list_zoom_level_memory(2);
        ensure!(
            storage.get_grid_zoom_level() == 3,
            "grid zoom must round-trip through the memory setter"
        );
        ensure!(
            storage.get_list_zoom_level() == 2,
            "list zoom must round-trip through the memory setter"
        );
        Ok(())
    }

    #[test]
    async fn volume_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_volume_memory(0.42);
        ensure!(
            (storage.get_settings_volume() - 0.42).abs() < f64::EPSILON,
            "volume must round-trip through the memory setter"
        );
        Ok(())
    }

    #[test]
    async fn output_mode_memory_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;

        storage.set_output_mode_memory(BitPerfect);
        ensure!(
            storage.get_output_mode() == BitPerfect,
            "output mode must round-trip through the memory setter"
        );
        Ok(())
    }
}

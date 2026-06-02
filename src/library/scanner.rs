//! Filesystem scanner that walks directories and discovers audio files.
//!
//! Implements the [`LibraryScanner`] trait for scanning configured library directories,
//! extracting metadata, deduplicating tracks, and persisting results to storage.

use std::{
    collections::HashMap,
    fs::{DirEntry, metadata, read_dir},
    io::Error,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use {
    async_channel::Sender,
    rayon::prelude::{IntoParallelRefIterator, ParallelIterator},
    tokio::{
        spawn,
        sync::{
            mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
            watch::{Receiver, Sender as TokioSender, channel},
        },
    },
    tracing::warn,
};

use crate::{
    library::{
        dedup::is_supported_audio_format,
        metadata::{AudioMetadata, extract_metadata, metadata_fingerprint},
        scanner::ScanEvent::{
            ScanCompleted, ScanProgress, ScanStarted, TrackDiscovered, TrackSkipped,
        },
    },
    storage::{NewAlbum, NewArtist, NewTrack, Storage, StorageError, TrackAudio},
};

/// Filesystem-based library scanner with storage integration.
pub struct FsScanner<S: Storage> {
    /// Storage backend for persistence.
    storage: Arc<S>,
    /// Configuration for the scanner.
    config: ScannerConfig,
    /// Cancellation signal sender.
    cancel_tx: TokioSender<bool>,
    /// Cancellation signal receiver (cloned into scan tasks).
    cancel_rx: Receiver<bool>,
    /// Channel sender for forwarding scan events to the UI.
    scan_event_tx: Sender<ScanEvent>,
}

impl<S: Storage> FsScanner<S> {
    /// Create a new filesystem scanner.
    #[must_use]
    pub fn new(storage: Arc<S>, config: ScannerConfig, scan_event_tx: Sender<ScanEvent>) -> Self {
        let (cancel_tx, cancel_rx) = channel(false);
        Self {
            storage,
            config,
            cancel_tx,
            cancel_rx,
            scan_event_tx,
        }
    }

    /// Walk a directory recursively and collect supported audio file paths.
    ///
    /// # Arguments
    ///
    /// * `dir` - Root directory to walk
    ///
    /// # Returns
    ///
    /// A vector of paths to supported audio files.
    fn walk_directory(dir: &Path) -> Vec<PathBuf> {
        let mut results = Vec::new();
        Self::walk_recursive(dir, &mut results);
        results
    }

    /// Walk a directory recursively in parallel using rayon.
    ///
    /// # Arguments
    ///
    /// * `dir` - Root directory to walk
    ///
    /// # Returns
    ///
    /// A vector of paths to supported audio files.
    fn walk_directory_parallel(dir: &Path) -> Vec<PathBuf> {
        let entries: Vec<_> = read_dir(dir).into_iter().flatten().flatten().collect();

        let mut results: Vec<PathBuf> = Vec::new();
        let mut subdirs: Vec<PathBuf> = Vec::new();

        for entry in &entries {
            Self::classify_entry(entry, &mut subdirs, &mut results);
        }

        let sub_results: Vec<Vec<PathBuf>> = subdirs
            .par_iter()
            .map(|path| Self::walk_directory_parallel(path))
            .collect();

        for sub_result in sub_results {
            results.extend(sub_result);
        }

        results
    }

    /// Classify a directory entry as a subdirectory or supported audio file.
    fn classify_entry(entry: &DirEntry, subdirs: &mut Vec<PathBuf>, results: &mut Vec<PathBuf>) {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
            return;
        }
        if path.is_file() && is_supported_audio_format(&path) {
            results.push(path);
        }
    }

    /// Process a single directory entry during recursive walk.
    fn walk_entry(entry: &DirEntry, results: &mut Vec<PathBuf>) {
        let path = entry.path();
        let Ok(metadata) = metadata(&path) else {
            return;
        };

        if metadata.is_dir() {
            Self::walk_recursive(&path, results);
            return;
        }

        if metadata.is_file() && is_supported_audio_format(&path) {
            results.push(path);
        }
    }

    /// Recursively walk a directory, collecting supported audio files.
    fn walk_recursive(dir: &Path, results: &mut Vec<PathBuf>) {
        let Ok(entries) = read_dir(dir) else {
            let err = Error::last_os_error();
            warn!(dir = %dir.display(), error = %err, "Failed to read directory");
            return;
        };

        for entry in entries.flatten() {
            Self::walk_entry(&entry, results);
        }
    }

    /// Check if a file should be skipped based on path uniqueness.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the database lookup fails.
    async fn check_path_exists(&self, path: &Path) -> Result<bool, StorageError> {
        match self.storage.find_by_path(path).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if a file should be skipped based on content hash.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the database lookup fails.
    async fn check_hash_duplicate(&self, hash: &str) -> Result<bool, StorageError> {
        match self.storage.find_by_hash(hash).await {
            Ok(tracks) => Ok(!tracks.is_empty()),
            Err(e) => Err(e),
        }
    }

    /// Check if a file should be skipped based on metadata fingerprint.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the database lookup fails.
    async fn check_fingerprint_duplicate(
        &self,
        metadata: &AudioMetadata,
    ) -> Result<bool, StorageError> {
        let (artist, album, title, track) = metadata_fingerprint(metadata);
        let track_num = track.map(i32::cast_unsigned);
        match self
            .storage
            .find_by_metadata_fingerprint(&artist, &album, &title, track_num)
            .await
        {
            Ok(tracks) => Ok(!tracks.is_empty()),
            Err(e) => Err(e),
        }
    }

    /// Scan a single directory and emit events.
    async fn scan_dir(&self, dir: &Path, event_tx: &UnboundedSender<ScanEvent>) {
        if let Err(e) = event_tx.send(ScanStarted {
            directory: dir.to_path_buf(),
        }) {
            warn!(error = %e, "Failed to send ScanStarted event");
        }
        if let Err(e) = self
            .scan_event_tx
            .send(ScanStarted {
                directory: dir.to_path_buf(),
            })
            .await
        {
            warn!(error = %e, "Failed to send ScanStarted event");
        }

        let start = Instant::now();
        let files = Self::walk_directory_parallel(dir);
        let files_found = u32::try_from(files.len()).unwrap_or(0);

        let extracted: Vec<_> = files
            .par_iter()
            .filter_map(|path| match extract_metadata(path) {
                Ok(metadata) => Some((path.clone(), metadata)),
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to extract metadata");
                    None
                }
            })
            .collect();

        let total = extracted.len();

        let mut artist_cache: HashMap<String, i64> = HashMap::new();
        let mut album_cache: HashMap<(i64, String), i64> = HashMap::new();

        let artists = self.storage.get_all_artists().await.unwrap_or_default();
        for a in &artists {
            artist_cache.insert(a.name.to_lowercase(), a.id);
        }

        let mut tracks_added: u64 = 0;
        let mut tracks_skipped: u64 = 0;

        for (idx, (path, metadata)) in extracted.into_iter().enumerate() {
            let mut ctx = ScanContext {
                dir,
                files_found,
                event_tx,
                artist_cache: &mut artist_cache,
                album_cache: &mut album_cache,
                tracks_added: &mut tracks_added,
                tracks_skipped: &mut tracks_skipped,
            };
            self.process_scan_item(idx, total, path, metadata, &mut ctx)
                .await;
        }

        if let Err(e) = event_tx.send(ScanCompleted {
            directory: dir.to_path_buf(),
            duration: start.elapsed(),
            tracks_added,
            tracks_skipped,
        }) {
            warn!(error = %e, "Failed to send ScanCompleted event");
        }
        if let Err(e) = self
            .scan_event_tx
            .send(ScanCompleted {
                directory: dir.to_path_buf(),
                duration: start.elapsed(),
                tracks_added,
                tracks_skipped,
            })
            .await
        {
            warn!(error = %e, "Failed to send ScanCompleted event");
        }
    }

    /// Process a single extracted item during directory scanning.
    async fn process_scan_item(
        &self,
        idx: usize,
        total: usize,
        path: PathBuf,
        metadata: AudioMetadata,
        ctx: &mut ScanContext<'_>,
    ) {
        if *self.cancel_rx.borrow() {
            return;
        }

        if (idx.is_multiple_of(100) || idx + 1 == total)
            && let Err(e) = ctx.event_tx.send(ScanProgress {
                directory: ctx.dir.to_path_buf(),
                files_found: ctx.files_found,
                files_processed: u32::try_from(idx + 1).unwrap_or(0),
            })
        {
            warn!(error = %e, "Failed to send ScanProgress event");
        }
        if (idx.is_multiple_of(100) || idx + 1 == total)
            && let Err(e) = self
                .scan_event_tx
                .send(ScanProgress {
                    directory: ctx.dir.to_path_buf(),
                    files_found: ctx.files_found,
                    files_processed: u32::try_from(idx + 1).unwrap_or(0),
                })
                .await
        {
            warn!(error = %e, "Failed to send ScanProgress event");
        }

        match self
            .process_file_cached(
                &path,
                metadata,
                ctx.artist_cache,
                ctx.album_cache,
                ctx.event_tx,
            )
            .await
        {
            Ok(_) => *ctx.tracks_added += 1,
            Err(reason) => Self::handle_skipped(reason, path, ctx.tracks_skipped, ctx.event_tx),
        }
    }

    /// Handle a skipped track by incrementing the counter and emitting an event.
    fn handle_skipped(
        reason: SkipReason,
        path: PathBuf,
        tracks_skipped: &mut u64,
        event_tx: &UnboundedSender<ScanEvent>,
    ) {
        *tracks_skipped += 1;
        if let Err(e) = event_tx.send(TrackSkipped { path, reason }) {
            warn!(error = %e, "Failed to send TrackSkipped event");
        }
    }

    /// Map a storage insertion error to a skip reason with logging.
    fn map_insert_error(e: &StorageError, entity: &str) -> SkipReason {
        warn!(error = %e, "Failed to insert {entity}");
        SkipReason::CorruptFile
    }

    /// Resolve an artist ID from cache or by inserting into storage.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::CorruptFile` if the database insert fails.
    async fn resolve_artist(
        &self,
        name: &str,
        cache: &mut HashMap<String, i64>,
    ) -> Result<i64, SkipReason> {
        let key = name.to_lowercase();
        if let Some(&id) = cache.get(&key) {
            return Ok(id);
        }
        let id = self
            .storage
            .insert_artist(NewArtist {
                name: name.to_string(),
            })
            .await
            .map_err(|e| Self::map_insert_error(&e, "artist"))?;
        cache.insert(key, id);
        Ok(id)
    }

    /// Resolve an album ID from cache or by inserting into storage.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::CorruptFile` if the database insert fails.
    async fn resolve_album(
        &self,
        title: &str,
        artist_id: i64,
        metadata: &AudioMetadata,
        cache: &mut HashMap<(i64, String), i64>,
    ) -> Result<i64, SkipReason> {
        let key = (artist_id, title.to_lowercase());
        if let Some(&id) = cache.get(&key) {
            return Ok(id);
        }
        let sr = format_sample_rate(metadata.sample_rate);
        let codec_upper = metadata.codec.to_uppercase();
        let format_summary = metadata.bit_depth.map_or_else(
            || format!("{codec_upper}/{sr}"),
            |bd| format!("{codec_upper} {bd}/{sr}"),
        );
        let id = self
            .storage
            .insert_album(NewAlbum {
                title: title.to_string(),
                artist_id,
                year: metadata.year,
                genre: metadata.genre.clone(),
                artwork_path: None,
                format_summary,
                lossless: metadata.lossless,
            })
            .await
            .map_err(|e| Self::map_insert_error(&e, "album"))?;
        cache.insert(key, id);
        Ok(id)
    }

    /// Build track audio metadata from file path and extracted metadata.
    fn build_track_audio(
        path: &Path,
        metadata: &AudioMetadata,
        album_id: i64,
        artist_id: i64,
    ) -> TrackAudio {
        TrackAudio {
            file_path: path.to_string_lossy().to_string(),
            content_hash: None,
            format: metadata.codec.to_uppercase(),
            sample_rate: metadata.sample_rate,
            bit_depth: metadata.bit_depth,
            channels: metadata.channels,
            codec: metadata.codec.clone(),
            lossless: metadata.lossless,
            bitrate: metadata.bitrate,
            album_id: Some(album_id),
            artist_id: Some(artist_id),
            file_size: metadata.file_size,
            last_modified: utc_now_rfc3339(),
        }
    }

    /// Process a file using cached artist/album lookups to avoid repeated DB queries.
    ///
    /// # Errors
    ///
    /// Returns a `SkipReason` if the file is a duplicate, corrupt, or cannot be inserted.
    async fn process_file_cached(
        &self,
        path: &Path,
        metadata: AudioMetadata,
        artist_cache: &mut HashMap<String, i64>,
        album_cache: &mut HashMap<(i64, String), i64>,
        event_tx: &UnboundedSender<ScanEvent>,
    ) -> Result<TrackInfo, SkipReason> {
        if self.check_path_exists(path).await.map_err(|e| {
            warn!(error = %e, path = %path.display(), "Failed to check path existence");
            SkipReason::CorruptFile
        })? {
            return Err(SkipReason::DuplicateByPath);
        }

        if self
            .check_fingerprint_duplicate(&metadata)
            .await
            .map_err(|e| {
                warn!(error = %e, title = ?metadata.title, "Failed to check fingerprint duplicate");
                SkipReason::CorruptFile
            })?
        {
            return Err(SkipReason::DuplicateByFingerprint);
        }

        let artist_name = metadata.artist.as_deref().unwrap_or("Unknown Artist");
        let artist_id = self.resolve_artist(artist_name, artist_cache).await?;

        let album_title = metadata.album.as_deref().unwrap_or("Unknown Album");
        let album_id = self
            .resolve_album(album_title, artist_id, &metadata, album_cache)
            .await?;

        let track = NewTrack {
            title: metadata
                .title
                .clone()
                .unwrap_or_else(|| "Unknown Track".to_string()),
            track_number: metadata.track_number,
            disc_number: metadata.disc_number,
            duration: metadata.duration,
            audio: Self::build_track_audio(path, &metadata, album_id, artist_id),
        };

        let track_id = self.storage.insert_track(track).await.map_err(|e| {
            warn!(error = %e, "Failed to insert track");
            SkipReason::CorruptFile
        })?;

        let track_info = TrackInfo {
            id: track_id,
            metadata,
            path: path.to_path_buf(),
            content_hash: None,
            artist_id: Some(artist_id),
            album_id: Some(album_id),
        };

        if let Err(e) = event_tx.send(TrackDiscovered {
            track: Box::new(track_info.clone()),
        }) {
            warn!(error = %e, "Failed to send TrackDiscovered event");
        }

        Ok(track_info)
    }
}

impl<S: Storage + 'static> LibraryScanner for FsScanner<S> {
    async fn scan_all(&self) -> Result<(), StorageError> {
        let dirs = self.storage.list_library_directories().await?;
        let (event_tx, event_rx) = unbounded_channel();

        let event_handle = spawn(drain_events(event_rx));

        for dir in &dirs {
            let path = Path::new(&dir.path);
            self.scan_dir(path, &event_tx).await;
        }

        drop(event_tx);
        drop(event_handle);
        Ok(())
    }

    async fn scan_directory(&self, path: &Path) -> Result<(), StorageError> {
        let (event_tx, event_rx) = unbounded_channel();

        let event_handle = spawn(drain_events(event_rx));

        self.scan_dir(path, &event_tx).await;
        drop(event_tx);
        drop(event_handle);
        Ok(())
    }

    fn cancel(&self) -> Result<(), StorageError> {
        self.cancel_tx
            .send(true)
            .map_err(|e| StorageError::Database(format!("Failed to send cancel signal: {e}")))
    }
}

/// Controls and observes library scanning.
pub trait LibraryScanner: Send + 'static {
    /// Trigger a full scan of all configured directories.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the scan cannot be initiated.
    fn scan_all(&self) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Trigger a scan of a specific directory.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the scan cannot be initiated.
    fn scan_directory(&self, path: &Path) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Cancel any in-progress scan.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the cancellation signal cannot be sent.
    fn cancel(&self) -> Result<(), StorageError>;
}

/// Mutable state shared across scan item processing.
struct ScanContext<'a> {
    /// Directory being scanned.
    dir: &'a Path,
    /// Total files found in the directory.
    files_found: u32,
    /// Channel for emitting scan events.
    event_tx: &'a UnboundedSender<ScanEvent>,
    /// Cache of artist names to database IDs.
    artist_cache: &'a mut HashMap<String, i64>,
    /// Cache of (`artist_id`, `album_name`) to database IDs.
    album_cache: &'a mut HashMap<(i64, String), i64>,
    /// Counter for successfully added tracks.
    tracks_added: &'a mut u64,
    /// Counter for skipped tracks.
    tracks_skipped: &'a mut u64,
}

/// Events emitted during library scanning.
#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// Scan of a directory has started.
    ScanStarted {
        /// Directory being scanned.
        directory: PathBuf,
    },
    /// Progress update during scanning.
    ScanProgress {
        /// Directory being scanned.
        directory: PathBuf,
        /// Total files found so far.
        files_found: u32,
        /// Files processed so far.
        files_processed: u32,
    },
    /// A new track was discovered and added to storage.
    TrackDiscovered {
        /// The discovered track data.
        track: Box<TrackInfo>,
    },
    /// A track was skipped during scanning.
    TrackSkipped {
        /// Path of the skipped file.
        path: PathBuf,
        /// Reason the track was skipped.
        reason: SkipReason,
    },
    /// Scan of a directory completed.
    ScanCompleted {
        /// Directory that was scanned.
        directory: PathBuf,
        /// Duration of the scan.
        duration: Duration,
        /// Number of tracks added.
        tracks_added: u64,
        /// Number of tracks skipped.
        tracks_skipped: u64,
    },
    /// An error occurred during scanning.
    ScanError {
        /// Directory being scanned when error occurred.
        directory: PathBuf,
        /// The error message.
        error: String,
    },
}

/// Configuration for the library scanner.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Maximum number of concurrent metadata extractions.
    pub max_concurrent: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self { max_concurrent: 4 }
    }
}

/// Reason a track was skipped during scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// File extension not supported.
    UnsupportedFormat,
    /// File is corrupt or unreadable.
    CorruptFile,
    /// Duplicate detected by file path.
    DuplicateByPath,
    /// Duplicate detected by content hash.
    DuplicateByHash,
    /// Duplicate detected by metadata fingerprint.
    DuplicateByFingerprint,
}

/// Information about a discovered track.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    /// Database ID of the track (after insertion).
    pub id: i64,
    /// Extracted metadata.
    pub metadata: AudioMetadata,
    /// Absolute path to the audio file.
    pub path: PathBuf,
    /// SHA-256 content hash.
    pub content_hash: Option<String>,
    /// Database ID of the artist (after insertion).
    pub artist_id: Option<i64>,
    /// Database ID of the album (after insertion).
    pub album_id: Option<i64>,
}

/// Drain events from a channel until it is closed.
async fn drain_events(mut event_rx: UnboundedReceiver<ScanEvent>) {
    while event_rx.recv().await.is_some() {}
}

/// Format sample rate for display in Hz.
///
/// Converts to kHz-style value: 44100 → "44.1", 48000 → "48".
fn format_sample_rate(hz: i32) -> String {
    if hz % 1000 == 0 {
        (hz / 1000).to_string()
    } else {
        format!("{:.1}", f64::from(hz) / 1000.0)
    }
}

/// Get the current UTC time as an RFC 3339 formatted string.
///
/// # Panics
///
/// Panics if the system time is before UNIX epoch (should never happen in practice).
fn utc_now_rfc3339() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since UNIX epoch to (year, month, day).
///
/// # Panics
///
/// Panics if the date calculation overflows (should not happen for reasonable dates).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir, write},
        path::PathBuf,
    };

    use {
        anyhow::{Result, bail},
        tempfile::tempdir,
    };

    use crate::{
        library::scanner::{
            FsScanner,
            ScanEvent::{ScanStarted, TrackSkipped},
            SkipReason::{
                CorruptFile, DuplicateByFingerprint, DuplicateByHash, DuplicateByPath,
                UnsupportedFormat,
            },
        },
        storage::database::SqliteStorage,
    };

    #[test]
    fn walk_directory_finds_audio_files() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path();

        write(root.join("track1.flac"), b"")?;
        write(root.join("track2.mp3"), b"")?;
        write(root.join("track3.wav"), b"")?;
        write(root.join("readme.txt"), b"")?;
        write(root.join("image.jpg"), b"")?;

        let sub = root.join("subdir");
        create_dir(&sub)?;
        write(sub.join("nested.flac"), b"")?;

        let files = FsScanner::<SqliteStorage>::walk_directory(root);
        if files.len() != 4 {
            bail!("expected 4 audio files, got {}", files.len());
        }
        Ok(())
    }

    #[test]
    fn walk_directory_handles_empty() -> Result<()> {
        let dir = tempdir()?;
        let files = FsScanner::<SqliteStorage>::walk_directory(dir.path());
        if !files.is_empty() {
            bail!("expected empty directory, got {} files", files.len());
        }
        Ok(())
    }

    #[test]
    fn scan_event_variants() {
        let started = ScanStarted {
            directory: PathBuf::from("/music"),
        };
        assert!(matches!(started, ScanStarted { .. }));

        let skipped = TrackSkipped {
            path: PathBuf::from("/music/bad.flac"),
            reason: UnsupportedFormat,
        };
        assert!(matches!(skipped, TrackSkipped { .. }));
    }

    #[test]
    fn skip_reason_equality() {
        assert_eq!(DuplicateByPath, DuplicateByPath);
        assert_eq!(CorruptFile, CorruptFile);
        assert_ne!(DuplicateByHash, DuplicateByFingerprint);
    }
}

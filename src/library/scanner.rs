//! Filesystem scanner that walks directories and discovers audio files.
//!
//! Implements the [`LibraryScanner`] trait for scanning configured library directories,
//! extracting metadata, deduplicating tracks, and persisting results to storage.

use std::{
    fs::{DirEntry, metadata, read_dir},
    io::Error,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use {
    tokio::{
        spawn,
        sync::{
            mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
            watch::{Receiver, Sender, channel},
        },
    },
    tracing::warn,
};

use crate::{
    library::{
        dedup::{compute_content_hash, is_supported_audio_format},
        metadata::{
            AudioMetadata,
            MetadataError::{FileNotFound, InvalidDuration, ReadError},
            extract_metadata, metadata_fingerprint,
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
    cancel_tx: Sender<bool>,
    /// Cancellation signal receiver (cloned into scan tasks).
    cancel_rx: Receiver<bool>,
}

impl<S: Storage> FsScanner<S> {
    /// Create a new filesystem scanner.
    #[must_use]
    pub fn new(storage: Arc<S>, config: ScannerConfig) -> Self {
        let (cancel_tx, cancel_rx) = channel(false);
        Self {
            storage,
            config,
            cancel_tx,
            cancel_rx,
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
    async fn check_path_exists(&self, path: &Path) -> Result<bool, StorageError> {
        match self.storage.find_by_path(path).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if a file should be skipped based on content hash.
    async fn check_hash_duplicate(&self, hash: &str) -> Result<bool, StorageError> {
        match self.storage.find_by_hash(hash).await {
            Ok(tracks) => Ok(!tracks.is_empty()),
            Err(e) => Err(e),
        }
    }

    /// Check if a file should be skipped based on metadata fingerprint.
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

    /// Get or create an artist in storage.
    async fn get_or_create_artist(&self, name: &str) -> Result<i64, StorageError> {
        let artist_name = if name.is_empty() {
            "Unknown Artist"
        } else {
            name
        };

        let artists = self.storage.get_all_artists().await?;
        if let Some(artist) = artists
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(artist_name))
        {
            return Ok(artist.id);
        }

        self.storage
            .insert_artist(NewArtist {
                name: artist_name.to_string(),
            })
            .await
    }

    /// Get or create an album in storage.
    async fn get_or_create_album(
        &self,
        metadata: &AudioMetadata,
        artist_id: i64,
    ) -> Result<i64, StorageError> {
        let album_title = metadata.album.as_deref().unwrap_or("Unknown Album");

        let albums = self.storage.get_albums_by_artist(artist_id).await?;
        if let Some(album) = albums
            .iter()
            .find(|a| a.title.eq_ignore_ascii_case(album_title))
        {
            return Ok(album.id);
        }

        let format_summary = format!(
            "{}{}/{}",
            metadata.codec.to_uppercase(),
            metadata
                .bit_depth
                .map_or(String::new(), |bd| format!(" {bd}-bit")),
            metadata.sample_rate,
        );

        self.storage
            .insert_album(NewAlbum {
                title: album_title.to_string(),
                artist_id,
                year: metadata.year,
                genre: metadata.genre.clone(),
                artwork_path: None,
                format_summary,
                lossless: metadata.lossless,
            })
            .await
    }

    /// Process a single audio file: extract metadata, dedup, and store.
    async fn process_file(
        &self,
        path: &Path,
        event_tx: &UnboundedSender<ScanEvent>,
    ) -> Result<TrackInfo, SkipReason> {
        let metadata = extract_metadata(path).map_err(|e| match e {
            ReadError(_) | FileNotFound(_) | InvalidDuration(_) => SkipReason::CorruptFile,
        })?;

        if self.check_path_exists(path).await.map_err(|e| {
            warn!(error = %e, path = %path.display(), "Failed to check path existence");
            SkipReason::CorruptFile
        })? {
            return Err(SkipReason::DuplicateByPath);
        }

        let content_hash = compute_content_hash(path).map_err(|e| {
            warn!(error = %e, path = %path.display(), "Failed to compute content hash");
            SkipReason::CorruptFile
        })?;

        if self
            .check_hash_duplicate(&content_hash)
            .await
            .map_err(|e| {
                warn!(error = %e, hash = %content_hash, "Failed to check hash duplicate");
                SkipReason::CorruptFile
            })?
        {
            return Err(SkipReason::DuplicateByHash);
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

        let artist_id = self
            .get_or_create_artist(metadata.artist.as_deref().unwrap_or("Unknown Artist"))
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to get or create artist");
                SkipReason::CorruptFile
            })?;

        let album_id = self
            .get_or_create_album(&metadata, artist_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to get or create album");
                SkipReason::CorruptFile
            })?;

        let track = NewTrack {
            title: metadata
                .title
                .clone()
                .unwrap_or_else(|| "Unknown Track".to_string()),
            track_number: metadata.track_number,
            disc_number: metadata.disc_number,
            duration: metadata.duration,
            audio: TrackAudio {
                file_path: path.to_string_lossy().to_string(),
                content_hash: Some(content_hash.clone()),
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
            },
        };

        let track_id = self.storage.insert_track(track).await.map_err(|e| {
            warn!(error = %e, "Failed to insert track");
            SkipReason::CorruptFile
        })?;

        let track_info = TrackInfo {
            id: track_id,
            metadata,
            path: path.to_path_buf(),
            content_hash: Some(content_hash),
            artist_id: Some(artist_id),
            album_id: Some(album_id),
        };

        if let Err(e) = event_tx.send(ScanEvent::TrackDiscovered {
            track: Box::new(track_info.clone()),
        }) {
            warn!(error = %e, "Failed to send TrackDiscovered event");
        }

        Ok(track_info)
    }

    /// Process a single file during directory scanning.
    async fn process_scan_entry(&self, idx: usize, path: &Path, ctx: &mut ScanContext<'_>) {
        if *self.cancel_rx.borrow() {
            return;
        }

        if idx.is_multiple_of(100)
            && let Err(e) = ctx.event_tx.send(ScanEvent::ScanProgress {
                directory: ctx.dir.to_path_buf(),
                files_found: ctx.files_found,
                files_processed: u64::try_from(idx).unwrap_or(0),
            })
        {
            warn!(error = %e, "Failed to send ScanProgress event");
        }

        let mut maybe_reason = None;
        match self.process_file(path, ctx.event_tx).await {
            Ok(_) => *ctx.tracks_added += 1,
            Err(reason) => {
                *ctx.tracks_skipped += 1;
                maybe_reason = Some(reason);
            }
        }
        if let Some(reason) = maybe_reason
            && let Err(e) = ctx.event_tx.send(ScanEvent::TrackSkipped {
                path: path.to_path_buf(),
                reason,
            })
        {
            warn!(error = %e, "Failed to send TrackSkipped event");
        }
    }

    /// Scan a single directory and emit events.
    async fn scan_dir(&self, dir: &Path, event_tx: &UnboundedSender<ScanEvent>) {
        if let Err(e) = event_tx.send(ScanEvent::ScanStarted {
            directory: dir.to_path_buf(),
        }) {
            warn!(error = %e, "Failed to send ScanStarted event");
        }

        let start = Instant::now();
        let files = Self::walk_directory(dir);
        let files_found = u64::try_from(files.len()).unwrap_or(0);
        let mut tracks_added: u64 = 0;
        let mut tracks_skipped: u64 = 0;

        let mut ctx = ScanContext {
            dir,
            files_found,
            event_tx,
            tracks_added: &mut tracks_added,
            tracks_skipped: &mut tracks_skipped,
        };

        for (idx, path) in files.iter().enumerate() {
            self.process_scan_entry(idx, path, &mut ctx).await;
        }

        if let Err(e) = event_tx.send(ScanEvent::ScanCompleted {
            directory: dir.to_path_buf(),
            duration: start.elapsed(),
            tracks_added,
            tracks_skipped,
        }) {
            warn!(error = %e, "Failed to send ScanCompleted event");
        }
    }
}

impl<S: Storage + 'static> LibraryScanner for FsScanner<S> {
    async fn scan_all(&self) -> Result<(), StorageError> {
        let dirs = self.storage.list_library_directories().await?;
        let (event_tx, event_rx) = unbounded_channel();

        let _event_handle = spawn(drain_events(event_rx));

        for dir in &dirs {
            let path = Path::new(&dir.path);
            self.scan_dir(path, &event_tx).await;
        }

        Ok(())
    }

    async fn scan_directory(&self, path: &Path) -> Result<(), StorageError> {
        let (event_tx, event_rx) = unbounded_channel();

        let _event_handle = spawn(drain_events(event_rx));

        self.scan_dir(path, &event_tx).await;
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

/// Mutable state shared across scan entry processing.
struct ScanContext<'a> {
    /// Directory being scanned.
    dir: &'a Path,
    /// Total files found in the directory.
    files_found: u64,
    /// Channel sender for scan events.
    event_tx: &'a UnboundedSender<ScanEvent>,
    /// Counter for tracks successfully added.
    tracks_added: &'a mut u64,
    /// Counter for tracks skipped during scanning.
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
        files_found: u64,
        /// Files processed so far.
        files_processed: u64,
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

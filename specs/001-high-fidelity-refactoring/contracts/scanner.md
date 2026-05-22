# Scanner Interface Contract

## Purpose

The library scanner discovers audio files in configured directories, extracts metadata,
deduplicates, and persists results to storage.

## Trait

```rust
/// Controls and observes library scanning.
pub trait LibraryScanner: Send + 'static {
    /// Trigger a full scan of all configured directories.
    fn scan_all(&self) -> Result<()>;
    /// Trigger a scan of a specific directory.
    fn scan_directory(&self, path: &Path) -> Result<()>;
    /// Cancel any in-progress scan.
    fn cancel(&self) -> Result<()>;
}
```

## Events

```rust
/// Events emitted during library scanning.
pub enum ScanEvent {
    ScanStarted { directory: PathBuf },
    ScanProgress {
        directory: PathBuf,
        files_found: u64,
        files_processed: u64,
    },
    TrackDiscovered {
        track: Track,
        album: Option<Album>,
        artist: Option<Artist>,
    },
    TrackSkipped {
        path: PathBuf,
        reason: SkipReason,
    },
    ScanCompleted {
        directory: PathBuf,
        duration: Duration,
        tracks_added: u64,
        tracks_skipped: u64,
    },
    ScanError {
        directory: PathBuf,
        error: ScanError,
    },
}

pub enum SkipReason {
    UnsupportedFormat,
    CorruptFile,
    DuplicateByPath,
    DuplicateByHash,
    DuplicateByFingerprint,
}
```

## Scan Algorithm

1. Walk configured directory recursively (using `walkdir` or `jwalk` for performance)
2. Filter by extension: `.flac`, `.mp3`, `.aac`/`.m4a`, `.ogg`, `.opus`, `.wav`, `.aiff`/`.aif`
3. For each file:
   a. Check `file_path` uniqueness in database → skip if exists with same mtime
   b. Compute SHA-256 content hash → check for hash collision → skip if match
   c. Parse metadata with `lofty` → fallback to filename-derived values
   d. Build metadata fingerprint (artist+album+title+track) → check for collision → skip if match
   e. Insert track, album (create if new), artist (create if new) into storage
4. Emit `TrackDiscovered` event → UI updates incrementally
5. On completion, emit `ScanCompleted` event

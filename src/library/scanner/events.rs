//! Scan events, skip reasons, and discovered-track data.

use std::{path::PathBuf, time::Duration};

use crate::library::metadata::AudioMetadata;

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::library::scanner::events::{
        ScanEvent::{ScanStarted, TrackSkipped},
        SkipReason::{
            CorruptFile, DuplicateByFingerprint, DuplicateByHash, DuplicateByPath,
            UnsupportedFormat,
        },
    };

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

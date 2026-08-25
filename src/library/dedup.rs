//! Layered duplicate detection: path uniqueness, content hash, metadata fingerprint.

use std::{
    fs::File,
    io::{Error as IoError, Read},
    path::Path,
};

use {
    hex::encode,
    sha2::{Digest, Sha256},
    thiserror::Error,
};

use crate::library::metadata::AudioMetadata;

/// Errors occurring during deduplication checks.
#[derive(Debug, Error)]
pub enum DedupError {
    /// Failed to read file for hashing.
    #[error("Failed to read file for hashing: {0}")]
    IoError(#[from] IoError),
    /// Hash computation failed.
    #[error("Hash computation error: {0}")]
    HashError(String),
}

/// Result of a deduplication check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupResult {
    /// No duplicate found; track is unique.
    Unique,
    /// Duplicate detected by exact file path match.
    DuplicateByPath,
    /// Duplicate detected by SHA-256 content hash collision.
    DuplicateByHash(String),
    /// Duplicate detected by metadata fingerprint match.
    DuplicateByFingerprint,
}

/// Compute SHA-256 hash of a file's contents.
///
/// # Arguments
///
/// * `path` - Path to the file to hash
///
/// # Returns
///
/// A `Result` containing the hex-encoded SHA-256 hash string.
///
/// # Errors
///
/// Returns [`DedupError`] if the file cannot be read.
pub fn compute_content_hash(path: &Path) -> Result<String, DedupError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        let Some(chunk) = buffer.get(..bytes_read) else {
            continue;
        };
        hasher.update(chunk);
    }

    Ok(encode(hasher.finalize()))
}

/// Create a metadata fingerprint string for duplicate comparison.
///
/// The fingerprint is a concatenation of normalized artist, album, title,
/// and track number fields.
#[must_use]
pub fn create_fingerprint(metadata: &AudioMetadata) -> String {
    let artist = metadata.artist.as_deref().unwrap_or("unknown_artist");
    let album = metadata.album.as_deref().unwrap_or("unknown_album");
    let title = metadata.title.as_deref().unwrap_or("unknown_track");
    let track = metadata
        .track_number
        .map_or(String::new(), |n| n.to_string());

    format!("{artist}|{album}|{title}|{track}")
}

/// Check if a file path indicates a supported audio format.
///
/// Supported extensions: `.flac`, `.mp3`, `.aac`, `.m4a`, `.ogg`, `.opus`,
/// `.wav`, `.aiff`, `.aif`.
#[must_use]
pub fn is_supported_audio_format(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                "flac" | "mp3" | "aac" | "m4a" | "ogg" | "opus" | "wav" | "aiff" | "aif"
            )
        })
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        path::{Path, PathBuf},
    };

    use {
        anyhow::{Result, bail, ensure},
        tempfile::NamedTempFile,
    };

    use crate::library::{
        dedup::{compute_content_hash, create_fingerprint, is_supported_audio_format},
        metadata::tests::{test_metadata, test_metadata_defaults},
    };

    #[test]
    fn supported_audio_formats() {
        assert!(is_supported_audio_format(&PathBuf::from("track.flac")));
        assert!(is_supported_audio_format(&PathBuf::from("track.mp3")));
        assert!(is_supported_audio_format(&PathBuf::from("track.m4a")));
        assert!(is_supported_audio_format(&PathBuf::from("track.aac")));
        assert!(is_supported_audio_format(&PathBuf::from("track.ogg")));
        assert!(is_supported_audio_format(&PathBuf::from("track.opus")));
        assert!(is_supported_audio_format(&PathBuf::from("track.wav")));
        assert!(is_supported_audio_format(&PathBuf::from("track.aiff")));
        assert!(is_supported_audio_format(&PathBuf::from("track.aif")));
    }

    #[test]
    fn unsupported_audio_formats() {
        assert!(!is_supported_audio_format(&PathBuf::from("track.txt")));
        assert!(!is_supported_audio_format(&PathBuf::from("track.jpg")));
        assert!(!is_supported_audio_format(&PathBuf::from("track")));
    }

    #[test]
    fn create_fingerprint_normalizes() {
        let meta = test_metadata();
        let fp = create_fingerprint(&meta);
        assert!(fp.contains("Some Artist"));
        assert!(fp.contains("Some Album"));
        assert!(fp.contains("My Track"));
        assert!(fp.contains('3'));
    }

    #[test]
    fn create_fingerprint_defaults() {
        let meta = test_metadata_defaults();
        let fp = create_fingerprint(&meta);
        assert!(fp.contains("unknown_artist"));
        assert!(fp.contains("unknown_album"));
        assert!(fp.contains("unknown_track"));
    }

    #[test]
    fn compute_content_hash_matches_known_digest() -> Result<()> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(b"hello world")?;
        let hash = compute_content_hash(tmp.path())?;
        ensure!(
            hash == "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            "unexpected digest: {hash}"
        );
        Ok(())
    }

    #[test]
    fn compute_content_hash_missing_file_errors() -> Result<()> {
        let result = compute_content_hash(Path::new("/nonexistent/file.flac"));
        if result.is_ok() {
            bail!("expected error for missing file");
        }
        Ok(())
    }
}

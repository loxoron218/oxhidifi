//! Parallel filesystem discovery and per-file metadata extraction.

use std::{
    fs::{DirEntry, read_dir},
    path::{Path, PathBuf},
};

use {
    rayon::{
        iter::IndexedParallelIterator,
        prelude::{IntoParallelRefIterator, ParallelIterator},
    },
    tokio::task::JoinError,
    tracing::{error, warn},
};

use crate::{
    library::{
        dedup::{compute_content_hash, is_supported_audio_format},
        metadata::{AudioMetadata, extract_metadata},
        scanner::FsScanner,
    },
    storage::Storage,
};

impl<S: Storage> FsScanner<S> {
    /// Walk a directory and extract metadata from all audio files found.
    #[must_use]
    pub fn walk_and_extract(
        dir: &Path,
        max_concurrent: usize,
        skip_hashing: bool,
    ) -> (Vec<(PathBuf, AudioMetadata, Option<String>)>, u32) {
        let files = Self::walk_directory_parallel(dir);
        let files_found = u32::try_from(files.len()).unwrap_or(0);

        let chunk_size = files.len().checked_div(max_concurrent).unwrap_or(0).max(1);

        let extracted: Vec<_> = files
            .par_iter()
            .with_min_len(chunk_size)
            .filter_map(|path| Self::extract_one(path, skip_hashing))
            .collect();

        (extracted, files_found)
    }

    /// Extract metadata and content hash from a single file path.
    fn extract_one(
        path: &Path,
        skip_hashing: bool,
    ) -> Option<(PathBuf, AudioMetadata, Option<String>)> {
        let metadata = extract_metadata(path).map_or_else(
            |e| {
                warn!(error = %e, path = %path.display(), "Failed to extract metadata");
                None
            },
            Some,
        )?;
        let content_hash = if skip_hashing {
            None
        } else {
            Self::try_compute_hash(path)
        };
        Some((path.to_path_buf(), metadata, content_hash))
    }

    /// Compute content hash for a file, logging on failure.
    fn try_compute_hash(path: &Path) -> Option<String> {
        compute_content_hash(path).map_or_else(
            |e| {
                warn!(error = %e, path = %path.display(), "Failed to compute content hash");
                None
            },
            Some,
        )
    }

    /// Log a panic from the walk-and-extract task and return defaults.
    pub fn on_walk_panic(e: &JoinError) -> (Vec<(PathBuf, AudioMetadata, Option<String>)>, u32) {
        error!(error = %e, "Walk and metadata extraction task panicked");
        Default::default()
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
    #[must_use]
    pub fn walk_directory_parallel(dir: &Path) -> Vec<PathBuf> {
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
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir, write};

    use {
        anyhow::{Result, bail},
        tempfile::tempdir,
    };

    use crate::{library::scanner::FsScanner, storage::database::SqliteStorage};

    #[test]
    fn walk_directory_finds_audio_files() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path();

        write(root.join("track1.flac"), b"\0")?;
        write(root.join("track2.mp3"), b"\0")?;
        write(root.join("track3.wav"), b"\0")?;
        write(root.join("readme.txt"), b"hello")?;
        write(root.join("image.jpg"), b"\0")?;

        let sub = root.join("subdir");
        create_dir(&sub)?;
        write(sub.join("nested.flac"), b"\0")?;

        let files = FsScanner::<SqliteStorage>::walk_directory_parallel(root);
        if files.len() != 4 {
            bail!("expected 4 audio files, got {}", files.len());
        }
        Ok(())
    }

    #[test]
    fn walk_directory_handles_empty() -> Result<()> {
        let dir = tempdir()?;
        let files = FsScanner::<SqliteStorage>::walk_directory_parallel(dir.path());
        if !files.is_empty() {
            bail!("expected empty directory, got {} files", files.len());
        }
        Ok(())
    }
}

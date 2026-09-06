//! Hash and duplicate screening helpers for scan ingestion.
//!
//! Extracted from `ingest.rs` to keep each file under the 400-line limit.

use std::path::{Path, PathBuf};

use {
    tokio::task::spawn_blocking,
    tracing::{info, warn},
};

use crate::{
    library::{
        dedup::compute_content_hash,
        metadata::AudioMetadata,
        scanner::{
            FsScanner,
            events::SkipReason::{
                self, CorruptFile, DuplicateByFingerprint, DuplicateByHash, DuplicateByPath,
            },
        },
    },
    storage::{
        Storage,
        catalog::{FieldUpdate, Track, TrackUpdate},
    },
};

impl<S: Storage> FsScanner<S> {
    /// Compute content hash off the async runtime to avoid blocking.
    pub async fn compute_hash_blocking(path: PathBuf) -> Option<String> {
        let path_clone = path.clone();
        let result = spawn_blocking(move || compute_content_hash(&path_clone)).await;
        match result {
            Ok(Ok(hash)) => Some(hash),
            Ok(Err(error)) => {
                warn!(
                    error = %error,
                    path = %path.display(),
                    "Failed to compute content hash"
                );
                None
            }
            Err(error) => {
                warn!(error = %error, path = %path.display(), "Content hash task panicked");
                None
            }
        }
    }

    /// Resolve rehashed content hash from precomputed value or blocking computation.
    pub async fn resolve_rehashed(path: &Path, content_hash: Option<String>) -> Option<String> {
        if let Some(hash) = content_hash {
            Some(hash)
        } else {
            Self::compute_hash_blocking(path.to_path_buf()).await
        }
    }

    /// Check whether a rehashed value is a duplicate in storage.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason` if the hash duplicate check fails.
    pub async fn check_rehashed_duplicate(
        &self,
        rehashed: Option<&String>,
    ) -> Result<bool, SkipReason> {
        let Some(hash) = rehashed else {
            return Ok(false);
        };
        self.check_hash_duplicate(hash)
            .await
            .map_err(|e| Self::on_hash_check_error(&e))
    }

    /// Determine collision kind from hash duplicate flag.
    #[must_use]
    pub const fn collision_kind(is_hash_dup: bool) -> SkipReason {
        if is_hash_dup {
            DuplicateByHash
        } else {
            DuplicateByPath
        }
    }

    /// Check whether a missing hash should be backfilled.
    #[must_use]
    pub const fn needs_backfill(existing_hash: Option<&String>, rehashed: Option<&String>) -> bool {
        existing_hash.is_none() && rehashed.is_some()
    }

    /// Backfill missing content hash for an existing track.
    pub async fn backfill_missing_hash(
        &self,
        track: &Track,
        rehashed: Option<&String>,
        path: &Path,
    ) {
        let Some(hash) = rehashed.cloned() else {
            return;
        };
        let update = TrackUpdate {
            content_hash: FieldUpdate::Set(hash),
            ..Default::default()
        };
        if let Err(error) = self.storage.update_track(track.id, update).await {
            warn!(
                error = %error,
                path = %path.display(),
                "Failed to backfill content_hash"
            );
        }
    }

    /// Delete an existing track, logging on failure.
    pub async fn delete_existing_track(&self, track_id: i64, path: &Path) {
        if let Err(error) = self.storage.delete_track(track_id).await {
            warn!(
                error = %error,
                path = %path.display(),
                "Failed to delete changed track"
            );
        }
    }

    /// Handle an existing track at `path`, returning pending hash state.
    ///
    /// # Arguments
    ///
    /// * `existing` - Track found at `path`, if any.
    /// * `path` - File path being processed.
    /// * `content_hash` - Precomputed hash from extraction.
    ///
    /// # Returns
    ///
    /// * `Ok((pending_hash, checked, is_dup))` - Pending hash and duplicate flags.
    /// * `Err(SkipReason)` - File should be skipped.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason` when the track is a duplicate or a hash check fails.
    pub async fn handle_existing_state(
        &self,
        existing: Option<Track>,
        path: &Path,
        content_hash: Option<String>,
    ) -> Result<(Option<String>, bool, bool), SkipReason> {
        let Some(track) = existing else {
            return Ok((None, false, false));
        };
        let rehashed = Self::resolve_rehashed(path, content_hash).await;
        let is_hash_dup = self.check_rehashed_duplicate(rehashed.as_ref()).await?;
        let collision = Self::collision_kind(is_hash_dup);
        let existing_hash = track.audio.content_hash.clone();
        if rehashed == existing_hash {
            return Err(collision);
        }
        if Self::needs_backfill(existing_hash.as_ref(), rehashed.as_ref()) {
            self.backfill_missing_hash(&track, rehashed.as_ref(), path)
                .await;
            return Err(collision);
        }
        info!(
            path = %path.display(),
            existing_hash = ?existing_hash,
            new_hash = ?rehashed,
            "File content changed, updating track"
        );
        self.delete_existing_track(track.id, path).await;
        if is_hash_dup {
            return Err(DuplicateByHash);
        }
        Ok((rehashed, true, is_hash_dup))
    }

    /// Validate that audio duration is positive.
    ///
    /// # Errors
    ///
    /// Returns `CorruptFile` when duration is zero or negative.
    pub fn ensure_valid_duration(metadata: &AudioMetadata, path: &Path) -> Result<(), SkipReason> {
        if metadata.duration > 0.0 {
            return Ok(());
        }
        warn!(
            path = %path.display(),
            duration = metadata.duration,
            "Skipping file with zero or negative duration \u{2014} corrupt audio data"
        );
        Err(CorruptFile)
    }

    /// Ensure fingerprint is not a duplicate.
    ///
    /// # Errors
    ///
    /// Returns `DuplicateByFingerprint` or `CorruptFile` on failure.
    pub async fn ensure_fingerprint_unique(
        &self,
        metadata: &AudioMetadata,
    ) -> Result<(), SkipReason> {
        let is_dup = self
            .check_fingerprint_duplicate(metadata)
            .await
            .map_err(|error| {
                warn!(
                    error = %error,
                    title = ?metadata.title,
                    "Failed to check fingerprint duplicate"
                );
                CorruptFile
            })?;
        if is_dup {
            return Err(DuplicateByFingerprint);
        }
        Ok(())
    }

    /// Resolve final hash after handling any existing track.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::DuplicateByHash` if the hash is a duplicate.
    pub async fn resolve_final_hash(
        &self,
        pending_hash: Option<String>,
        content_hash: Option<String>,
        checked: bool,
        is_dup: bool,
    ) -> Result<Option<String>, SkipReason> {
        if let Some(hash) = pending_hash {
            return Ok(Some(hash));
        }
        let Some(hash) = content_hash else {
            return Ok(None);
        };
        if checked && is_dup {
            return Err(DuplicateByHash);
        }
        if !checked {
            self.check_precomputed_hash(&hash).await?;
        }
        Ok(Some(hash))
    }
}

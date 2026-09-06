//! Skip and duplicate screening before a file is ingested.

use std::path::Path;

use tracing::{info, warn};

use crate::{
    library::{
        metadata::{AudioMetadata, metadata_fingerprint},
        scanner::{
            FsScanner,
            events::SkipReason::{self, CorruptFile, DuplicateByHash},
        },
    },
    storage::{Storage, StorageError},
};

impl<S: Storage> FsScanner<S> {
    /// Check if a file should be skipped based on path uniqueness.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the database lookup fails.
    pub async fn check_path_exists(&self, path: &Path) -> Result<bool, StorageError> {
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
    pub async fn check_hash_duplicate(&self, hash: &str) -> Result<bool, StorageError> {
        self.storage.hash_exists(hash).await
    }

    /// Check if a file should be skipped based on metadata fingerprint.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the database lookup fails.
    pub async fn check_fingerprint_duplicate(
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

    /// Log and return a skip reason for hash duplicate check failure.
    pub fn on_hash_check_error(e: &StorageError) -> SkipReason {
        warn!(error = %e, "Failed to check hash duplicate");
        CorruptFile
    }

    /// Check a pre-computed hash for duplicates.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::DuplicateByHash` if a duplicate is found.
    pub async fn check_precomputed_hash(&self, hash: &str) -> Result<(), SkipReason> {
        if self
            .check_hash_duplicate(hash)
            .await
            .map_err(|e| Self::on_hash_check_error(&e))?
        {
            return Err(DuplicateByHash);
        }
        Ok(())
    }

    /// Handle a skipped track by incrementing the counter.
    pub fn handle_skipped(reason: &SkipReason, path: &Path, tracks_skipped: &mut u64) {
        *tracks_skipped = tracks_skipped.saturating_add(1);
        info!(
            path = %path.display(),
            skip_reason = ?reason,
            "Track skipped",
        );
    }

    /// Map a storage insertion error to a skip reason with logging.
    pub fn map_insert_error(e: &StorageError, entity: &str) -> SkipReason {
        warn!(error = %e, "Failed to insert {entity}");
        CorruptFile
    }
}

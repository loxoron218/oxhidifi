//! Scan orchestration and database record insertion.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};

use {
    tokio::task::spawn_blocking,
    tracing::{info, warn},
};

use crate::{
    library::{
        metadata::AudioMetadata,
        scanner::{
            FsScanner,
            events::{
                ScanEvent::{ScanCompleted, ScanProgress, ScanStarted},
                SkipReason::{self, CorruptFile},
                TrackInfo,
            },
        },
    },
    storage::Storage,
};

impl<S: Storage> FsScanner<S> {
    /// Scan a single directory and emit events.
    pub async fn scan_dir(&self, dir: &Path) {
        info!(
            directory = %dir.display(),
            "Scan started",
        );

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

        let artists = self.storage.get_all_artists().await.unwrap_or_default();
        let skip_hashing = artists.is_empty();
        let mut artist_cache: HashMap<String, i64> = HashMap::with_capacity(artists.len());
        for a in &artists {
            artist_cache.insert(a.name.to_lowercase(), a.id);
        }
        let mut album_cache: HashMap<(i64, String), i64> = HashMap::new();

        let dir_buf = dir.to_path_buf();
        let max_concurrent = self.max_concurrent;
        let (extracted, files_found) = match spawn_blocking(move || {
            Self::walk_and_extract(&dir_buf, max_concurrent, skip_hashing)
        })
        .await
        {
            Ok(v) => v,
            Err(e) => Self::on_walk_panic(&e),
        };

        let total = extracted.len();

        let mut tracks_added: u64 = 0;
        let mut tracks_skipped: u64 = 0;

        for (idx, (path, metadata, content_hash)) in extracted.into_iter().enumerate() {
            let mut ctx = ScanContext {
                dir,
                files_found,
                artist_cache: &mut artist_cache,
                album_cache: &mut album_cache,
                tracks_added: &mut tracks_added,
                tracks_skipped: &mut tracks_skipped,
            };
            self.process_scan_item(idx, total, path, metadata, content_hash, &mut ctx)
                .await;
        }

        let duration = start.elapsed();
        let duration_seconds = duration.as_secs_f64();
        info!(
            directory = %dir.display(),
            tracks_added,
            tracks_skipped,
            duration_seconds,
            files_found,
            "Scan completed",
        );

        if let Err(e) = self
            .scan_event_tx
            .send(ScanCompleted {
                directory: dir.to_path_buf(),
                duration,
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
        content_hash: Option<String>,
        ctx: &mut ScanContext<'_>,
    ) {
        if *self.cancel_rx.borrow() {
            return;
        }

        let next_idx = idx.checked_add(1);
        if (idx.is_multiple_of(100) || next_idx == Some(total))
            && let Err(e) = self
                .scan_event_tx
                .send(ScanProgress {
                    directory: ctx.dir.to_path_buf(),
                    files_found: ctx.files_found,
                    files_processed: u32::try_from(next_idx.unwrap_or(0)).unwrap_or(0),
                })
                .await
        {
            warn!(error = %e, "Failed to send ScanProgress event");
        }

        match self
            .process_file_cached(
                &path,
                metadata,
                content_hash,
                ctx.artist_cache,
                ctx.album_cache,
            )
            .await
        {
            Ok(_) => *ctx.tracks_added = ctx.tracks_added.saturating_add(1),
            Err(reason) => Self::handle_skipped(&reason, &path, ctx.tracks_skipped),
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
        content_hash: Option<String>,
        artist_cache: &mut HashMap<String, i64>,
        album_cache: &mut HashMap<(i64, String), i64>,
    ) -> Result<TrackInfo, SkipReason> {
        Self::ensure_valid_duration(&metadata, path)?;
        let existing = self.storage.find_by_path(path).await.map_err(|error| {
            warn!(error = %error, path = %path.display(), "Failed to check path existence");
            CorruptFile
        })?;
        let (pending_hash, checked, is_dup) = self
            .handle_existing_state(existing, path, content_hash.clone())
            .await?;
        self.ensure_fingerprint_unique(&metadata).await?;
        let final_hash = self
            .resolve_final_hash(pending_hash, content_hash, checked, is_dup)
            .await?;
        let (album_id, track_artist_id) = self
            .resolve_track_ids(&metadata, path, artist_cache, album_cache)
            .await?;
        self.insert_track_record(path, metadata, album_id, track_artist_id, final_hash)
            .await
    }
}

/// Mutable state shared across scan item processing.
pub struct ScanContext<'a> {
    /// Directory being scanned.
    pub dir: &'a Path,
    /// Total files found in the directory.
    pub files_found: u32,
    /// Cache of artist names to database IDs.
    pub artist_cache: &'a mut HashMap<String, i64>,
    /// Cache of (`artist_id`, `album_name`) to database IDs.
    pub album_cache: &'a mut HashMap<(i64, String), i64>,
    /// Counter for successfully added tracks.
    pub tracks_added: &'a mut u64,
    /// Counter for skipped tracks.
    pub tracks_skipped: &'a mut u64,
}

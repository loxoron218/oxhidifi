//! Scan orchestration and database record insertion.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};

use {
    tokio::task::spawn_blocking,
    tracing::{error, info, warn},
};

use crate::{
    library::{
        artwork::{ArtworkError, cache_artwork, extract_artwork},
        metadata::AudioMetadata,
        scanner::{
            FsScanner,
            events::{
                ScanEvent::{ScanCompleted, ScanProgress, ScanStarted},
                SkipReason::{self, CorruptFile, DuplicateByFingerprint, DuplicateByPath},
                TrackInfo,
            },
            timefmt::{format_sample_rate, utc_now_rfc3339},
        },
    },
    storage::{
        Storage,
        catalog::{NewAlbum, NewArtist, NewTrack, TrackAudio},
    },
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

    /// Try to extract and cache artwork, returning the cached path string on success.
    fn cache_extracted_artwork(
        result: Result<Option<(Vec<u8>, String)>, ArtworkError>,
        key: &str,
    ) -> Option<String> {
        match result {
            Ok(Some((data, ext))) => match cache_artwork(key, &data, &ext) {
                Ok(p) => Some(p.to_string_lossy().to_string()),
                Err(e) => {
                    error!(error = %e, "Failed to cache artwork");
                    None
                }
            },
            _ => None,
        }
    }

    /// Resolve an album ID from cache or by inserting into storage.
    ///
    /// When a new album is inserted, embedded artwork is extracted from `file_path`,
    /// cached to disk, and the cached path is stored as `artwork_path`.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::CorruptFile` if the database insert fails.
    async fn resolve_album(
        &self,
        title: &str,
        artist_id: i64,
        file_path: &Path,
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
        let artwork_path = Self::cache_extracted_artwork(
            extract_artwork(file_path),
            &format!("{artist_id}_{}", title.to_lowercase()),
        );
        let id = self
            .storage
            .insert_album(NewAlbum {
                title: title.to_string(),
                artist_id,
                year: metadata.year,
                genre: metadata.genre.clone(),
                artwork_path,
                format_summary,
                lossless: metadata.lossless,
                format: codec_upper.clone(),
                bit_depth: metadata.bit_depth,
                sample_rate: Some(metadata.sample_rate),
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
        content_hash: Option<String>,
    ) -> TrackAudio {
        TrackAudio {
            file_path: path.to_string_lossy().to_string(),
            content_hash,
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
        content_hash: Option<String>,
        artist_cache: &mut HashMap<String, i64>,
        album_cache: &mut HashMap<(i64, String), i64>,
    ) -> Result<TrackInfo, SkipReason> {
        if metadata.duration <= 0.0 {
            warn!(
                path = %path.display(),
                duration = metadata.duration,
                "Skipping file with zero or negative duration \u{2014} corrupt audio data",
            );
            return Err(CorruptFile);
        }

        if self.check_path_exists(path).await.map_err(|e| {
            warn!(error = %e, path = %path.display(), "Failed to check path existence");
            CorruptFile
        })? {
            return Err(DuplicateByPath);
        }

        if self
            .check_fingerprint_duplicate(&metadata)
            .await
            .map_err(|e| {
                warn!(error = %e, title = ?metadata.title, "Failed to check fingerprint duplicate");
                CorruptFile
            })?
        {
            return Err(DuplicateByFingerprint);
        }

        let content_hash = match &content_hash {
            Some(h) => {
                self.check_precomputed_hash(h).await?;
                Some(h.clone())
            }
            None => None,
        };

        let album_artist_name = metadata
            .album_artist
            .as_deref()
            .or(metadata.artist.as_deref())
            .unwrap_or("Unknown Artist");
        let album_artist_id = self.resolve_artist(album_artist_name, artist_cache).await?;

        let album_title = metadata.album.as_deref().unwrap_or("Unknown Album");
        let album_id = self
            .resolve_album(album_title, album_artist_id, path, &metadata, album_cache)
            .await?;

        let track_artist_name = metadata.artist.as_deref().unwrap_or("Unknown Artist");
        let track_artist_id = if track_artist_name == album_artist_name {
            album_artist_id
        } else {
            self.resolve_artist(track_artist_name, artist_cache).await?
        };

        let track = NewTrack {
            title: metadata
                .title
                .clone()
                .unwrap_or_else(|| "Unknown Track".to_string()),
            track_number: metadata.track_number,
            disc_number: metadata.disc_number,
            duration: metadata.duration,
            audio: Self::build_track_audio(
                path,
                &metadata,
                album_id,
                track_artist_id,
                content_hash.clone(),
            ),
        };

        let track_id = self.storage.insert_track(track).await.map_err(|e| {
            warn!(error = %e, "Failed to insert track");
            CorruptFile
        })?;

        let track_info = TrackInfo {
            id: track_id,
            metadata,
            path: path.to_path_buf(),
            content_hash,
            artist_id: Some(track_artist_id),
            album_id: Some(album_id),
        };

        info!(
            track_id,
            album_id,
            artist_id = track_artist_id,
            path = %path.display(),
            "Track discovered",
        );

        Ok(track_info)
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

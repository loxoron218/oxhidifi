//! Artist and album resolution for scan ingestion.
//!
//! Extracted from `ingest.rs` to keep each file under the 400-line limit.

use std::{collections::HashMap, path::Path};

use tracing::{error, info, warn};

use crate::{
    library::{
        artwork::{ArtworkError, cache_artwork, extract_artwork},
        metadata::AudioMetadata,
        scanner::{
            FsScanner,
            events::{
                SkipReason::{self, CorruptFile},
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
    /// Resolve an artist ID from cache or by inserting into storage.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::CorruptFile` if the database insert fails.
    pub async fn resolve_artist(
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
    pub fn cache_extracted_artwork(
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
    pub async fn resolve_album(
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
        let genre = metadata
            .genre
            .clone()
            .or_else(|| Some("Unknown Genre".to_string()));
        let id = self
            .storage
            .insert_album(NewAlbum {
                title: title.to_string(),
                artist_id,
                year: metadata.year,
                genre,
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
    #[must_use]
    pub fn build_track_audio(
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

    /// Resolve album and track artist identifiers.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::CorruptFile` if the database insert fails.
    pub async fn resolve_track_ids(
        &self,
        metadata: &AudioMetadata,
        path: &Path,
        artist_cache: &mut HashMap<String, i64>,
        album_cache: &mut HashMap<(i64, String), i64>,
    ) -> Result<(i64, i64), SkipReason> {
        let album_artist_name = metadata
            .album_artist
            .as_deref()
            .or(metadata.artist.as_deref())
            .unwrap_or("Unknown Artist");
        let album_artist_id = self.resolve_artist(album_artist_name, artist_cache).await?;
        let album_title = metadata.album.as_deref().unwrap_or("Unknown Album");
        let album_id = self
            .resolve_album(album_title, album_artist_id, path, metadata, album_cache)
            .await?;
        let track_artist_name = metadata.artist.as_deref().unwrap_or("Unknown Artist");
        let track_artist_id = if track_artist_name == album_artist_name {
            album_artist_id
        } else {
            self.resolve_artist(track_artist_name, artist_cache).await?
        };
        Ok((album_id, track_artist_id))
    }

    /// Insert a new track record and return its info.
    ///
    /// # Errors
    ///
    /// Returns `SkipReason::CorruptFile` if the track cannot be inserted.
    pub async fn insert_track_record(
        &self,
        path: &Path,
        metadata: AudioMetadata,
        album_id: i64,
        artist_id: i64,
        content_hash: Option<String>,
    ) -> Result<TrackInfo, SkipReason> {
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
                artist_id,
                content_hash.clone(),
            ),
        };
        let track_id = self.storage.insert_track(track).await.map_err(|error| {
            warn!(error = %error, "Failed to insert track");
            CorruptFile
        })?;
        let info = TrackInfo {
            id: track_id,
            metadata: metadata.clone(),
            path: path.to_path_buf(),
            content_hash: content_hash.clone(),
            artist_id: Some(artist_id),
            album_id: Some(album_id),
        };
        info!(
            track_id,
            album_id,
            artist_id,
            path = %path.display(),
            "Track discovered"
        );
        Ok(info)
    }
}

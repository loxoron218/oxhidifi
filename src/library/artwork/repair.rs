//! Startup repair for stale album artwork paths.
//!
//! `check_cache_version` wipes the artwork cache directory on a format bump
//! while `albums.artwork_path` rows keep pointing at the deleted files. The
//! scanner skips already-indexed tracks, so those paths are never
//! re-extracted and every gallery build dispatches decodes for missing
//! files. This module re-extracts embedded art for such albums once at
//! startup and clears paths that cannot be repaired.

use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::{
    library::artwork::{artwork_cache_key, cache_artwork, extract_artwork},
    storage::Storage,
};

/// Outcome of repairing a single album.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlbumRepairOutcome {
    /// Cached file was re-extracted and the database row updated.
    Repaired,
    /// Stale path was cleared (no tracks, missing audio, or no embedded art).
    Cleared,
    /// Repair was attempted but failed (unreadable audio or cache write error).
    Failed,
    /// Nothing to do (path already exists or album has no stored path).
    Skipped,
}

/// Summary counts for a repair run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ArtworkRepairSummary {
    /// Albums examined.
    pub checked: u32,
    /// Albums whose artwork was re-extracted.
    pub repaired: u32,
    /// Albums whose stale path was cleared.
    pub cleared: u32,
    /// Albums that failed to repair.
    pub failed: u32,
}

/// Repair albums whose stored artwork file no longer exists.
///
/// For each album with a non-null `artwork_path` pointing at a missing file,
/// the first track's audio file is used to re-extract embedded art. Albums
/// with no tracks, missing audio files, or no embedded picture get their
/// path cleared so the UI stops dispatching decodes for them.
///
/// # Arguments
///
/// * `storage` - Storage backend providing albums and tracks.
///
/// # Returns
///
/// * `ArtworkRepairSummary` - Counts of checked, repaired, cleared, and failed albums.
pub async fn repair_missing_artwork<S: Storage>(storage: &S) -> ArtworkRepairSummary {
    let albums = match storage.get_all_albums().await {
        Ok(albums) => albums,
        Err(e) => {
            warn!(error = %e, "Failed to load albums for artwork repair");
            return ArtworkRepairSummary::default();
        }
    };

    let mut summary = ArtworkRepairSummary::default();
    for album in &albums {
        summary.checked = summary.checked.saturating_add(1);
        let outcome = repair_one_album(
            storage,
            album.id,
            &album.title,
            album.artist_id,
            album.artwork_path.as_deref(),
        )
        .await;
        match outcome {
            AlbumRepairOutcome::Repaired => {
                summary.repaired = summary.repaired.saturating_add(1);
            }
            AlbumRepairOutcome::Cleared => {
                summary.cleared = summary.cleared.saturating_add(1);
            }
            AlbumRepairOutcome::Failed => {
                summary.failed = summary.failed.saturating_add(1);
            }
            AlbumRepairOutcome::Skipped => {}
        }
    }

    info!(
        checked = summary.checked,
        repaired = summary.repaired,
        cleared = summary.cleared,
        failed = summary.failed,
        "Artwork repair completed"
    );
    summary
}

/// Repair a single album's artwork path.
///
/// Returns [`AlbumRepairOutcome::Skipped`] when there is nothing to do.
async fn repair_one_album<S: Storage>(
    storage: &S,
    album_id: i64,
    title: &str,
    artist_id: i64,
    artwork_path: Option<&str>,
) -> AlbumRepairOutcome {
    let Some(stored) = artwork_path else {
        return AlbumRepairOutcome::Skipped;
    };
    if Path::new(stored).exists() {
        return AlbumRepairOutcome::Skipped;
    }

    let Some(audio_path) = first_album_audio_path(storage, album_id).await else {
        clear_artwork(storage, album_id).await;
        return AlbumRepairOutcome::Cleared;
    };

    if !audio_path.exists() {
        warn!(
            album_id,
            path = %audio_path.display(),
            "Audio file missing, clearing stale artwork path"
        );
        clear_artwork(storage, album_id).await;
        return AlbumRepairOutcome::Cleared;
    }

    match extract_artwork(&audio_path) {
        Ok(Some((data, ext))) => {
            store_reextracted_artwork(storage, album_id, artist_id, title, &data, &ext).await
        }
        Ok(None) => {
            clear_artwork(storage, album_id).await;
            AlbumRepairOutcome::Cleared
        }
        Err(e) => {
            warn!(error = %e, album_id, "Failed to re-extract album artwork");
            AlbumRepairOutcome::Failed
        }
    }
}

/// Cache re-extracted artwork bytes and point the album row at the new file.
///
/// Returns [`AlbumRepairOutcome::Repaired`] on success and
/// [`AlbumRepairOutcome::Failed`] when the cache write or database update
/// fails.
async fn store_reextracted_artwork<S: Storage>(
    storage: &S,
    album_id: i64,
    artist_id: i64,
    title: &str,
    data: &[u8],
    ext: &str,
) -> AlbumRepairOutcome {
    let key = artwork_cache_key(artist_id, title);
    let cached = match cache_artwork(&key, data, ext) {
        Ok(cached) => cached,
        Err(e) => {
            warn!(error = %e, album_id, "Failed to cache re-extracted artwork");
            return AlbumRepairOutcome::Failed;
        }
    };
    let cached_str = cached.to_string_lossy().to_string();
    if let Err(e) = storage
        .update_album_artwork(album_id, Some(cached_str))
        .await
    {
        warn!(error = %e, album_id, "Failed to update repaired artwork path");
        return AlbumRepairOutcome::Failed;
    }
    info!(album_id, "Repaired missing album artwork");
    AlbumRepairOutcome::Repaired
}

/// Load the first track's audio file path for an album.
///
/// Returns `None` when the album has no tracks or the track query fails.
async fn first_album_audio_path<S: Storage>(storage: &S, album_id: i64) -> Option<PathBuf> {
    match storage.get_tracks_by_album(album_id).await {
        Ok(tracks) => tracks
            .first()
            .map(|track| PathBuf::from(&track.audio.file_path)),
        Err(e) => {
            warn!(error = %e, album_id, "Failed to load album tracks for repair");
            None
        }
    }
}

/// Clear an album's artwork path, logging on failure.
async fn clear_artwork<S: Storage>(storage: &S, album_id: i64) {
    if let Err(e) = storage.update_album_artwork(album_id, None).await {
        warn!(error = %e, album_id, "Failed to clear stale artwork path");
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::Path};

    use {
        anyhow::{Context, Result, ensure},
        sqlx::query,
        tempfile::{NamedTempFile, TempDir, tempdir},
        tokio::test,
    };

    use crate::{
        library::artwork::repair::repair_missing_artwork,
        storage::{
            Storage,
            catalog::{NewAlbum, NewArtist},
            database::SqliteStorage,
        },
    };

    async fn stale_album_fixture() -> Result<(TempDir, SqliteStorage, i64)> {
        let dir = tempdir()?;
        let storage = SqliteStorage::connect_with_settings_path(
            &dir.path().join("library.db"),
            &dir.path().join("settings.json"),
        )
        .await
        .context("storage should connect")?;
        let artist_id = storage
            .insert_artist(NewArtist {
                name: "Repair Artist".to_string(),
            })
            .await?;
        let album_id = storage
            .insert_album(NewAlbum {
                title: "Stale Album".to_string(),
                artist_id,
                year: Some(2024),
                genre: Some("Rock".to_string()),
                artwork_path: Some("/nonexistent-oxhidifi-cover.jpg".to_string()),
                format_summary: "FLAC".to_string(),
                lossless: true,
                format: "FLAC".to_string(),
                bit_depth: Some(16),
                sample_rate: Some(44100),
            })
            .await?;
        Ok((dir, storage, album_id))
    }

    async fn insert_track(storage: &SqliteStorage, audio_path: &Path, album_id: i64) -> Result<()> {
        _ = query(
            "INSERT INTO tracks (title, duration, file_path, format, sample_rate, channels, \
             codec, lossless, album_id, file_size, last_modified) VALUES (?, ?, ?, ?, ?, ?, ?, ?, \
             ?, ?, ?)",
        )
        .bind("Repair Track")
        .bind(180.0)
        .bind(audio_path.to_string_lossy().to_string())
        .bind("FLAC")
        .bind(44100)
        .bind(2)
        .bind("flac")
        .bind(true)
        .bind(album_id)
        .bind(1024)
        .bind("2024-01-01T00:00:00Z")
        .execute(&storage.pool)
        .await?;
        Ok(())
    }

    async fn stored_artwork(storage: &SqliteStorage, album_id: i64) -> Result<Option<String>> {
        Ok(storage
            .get_album(album_id)
            .await?
            .context("album should exist")?
            .artwork_path)
    }

    #[test]
    async fn clears_stale_path_when_album_has_no_tracks() -> Result<()> {
        let (dir, storage, album_id) = stale_album_fixture().await?;
        let summary = repair_missing_artwork(&storage).await;
        ensure!(summary.checked == 1, "one album should be checked");
        ensure!(summary.cleared == 1, "stale path should be cleared");
        ensure!(
            summary.repaired == 0 && summary.failed == 0,
            "nothing else should happen"
        );
        ensure!(
            stored_artwork(&storage, album_id).await?.is_none(),
            "stale artwork path should be cleared"
        );
        drop(dir);
        Ok(())
    }

    #[test]
    async fn leaves_existing_artwork_file_untouched() -> Result<()> {
        let (dir, storage, album_id) = stale_album_fixture().await?;
        let cover = NamedTempFile::new()?;
        let cover_path = cover.path().to_string_lossy().to_string();
        storage
            .update_album_artwork(album_id, Some(cover_path.clone()))
            .await?;
        let summary = repair_missing_artwork(&storage).await;
        ensure!(summary.checked == 1, "one album should be checked");
        ensure!(
            summary.cleared == 0 && summary.repaired == 0 && summary.failed == 0,
            "existing artwork must be left untouched"
        );
        ensure!(
            stored_artwork(&storage, album_id).await?.as_deref() == Some(cover_path.as_str()),
            "existing artwork path must be preserved"
        );
        drop(dir);
        Ok(())
    }

    #[test]
    async fn counts_failure_when_audio_is_unreadable() -> Result<()> {
        let (dir, storage, album_id) = stale_album_fixture().await?;
        let mut audio = NamedTempFile::new()?;
        audio.write_all(b"not an audio file")?;
        insert_track(&storage, audio.path(), album_id).await?;
        let summary = repair_missing_artwork(&storage).await;
        ensure!(summary.checked == 1, "one album should be checked");
        ensure!(
            summary.failed == 1,
            "unreadable audio should count as failed"
        );
        ensure!(
            stored_artwork(&storage, album_id).await?.is_some(),
            "failed repair must preserve the path for a later retry"
        );
        drop(dir);
        Ok(())
    }

    #[test]
    async fn clears_stale_path_when_audio_file_is_gone() -> Result<()> {
        let (dir, storage, album_id) = stale_album_fixture().await?;
        insert_track(
            &storage,
            Path::new("/nonexistent-oxhidifi-audio.flac"),
            album_id,
        )
        .await?;
        let summary = repair_missing_artwork(&storage).await;
        ensure!(
            summary.cleared == 1,
            "missing audio should clear the stale path"
        );
        ensure!(
            stored_artwork(&storage, album_id).await?.is_none(),
            "stale path should be cleared when audio is gone"
        );
        drop(dir);
        Ok(())
    }
}

//! Shared storage test scaffolding: temp storage and track fixtures.

use std::path::Path;

use {
    anyhow::{Context, Result},
    tempfile::{TempDir, tempdir},
};

use oxhidifi::storage::{
    catalog::{NewAlbum, NewTrack, TrackAudio},
    database::SqliteStorage,
};

async fn storage_connect(dir: &Path) -> Result<SqliteStorage> {
    let db_path = dir.join("test.db");
    let settings_path = dir.join("settings.json");
    SqliteStorage::connect_with_settings_path(&db_path, &settings_path)
        .await
        .context("failed to connect to storage")
}

/// Create a temporary `SqliteStorage` instance for testing.
///
/// # Errors
///
/// Returns an error if the temp directory or database connection cannot be created.
pub async fn test_storage() -> Result<(SqliteStorage, TempDir)> {
    let dir = tempdir().context("failed to create temp dir")?;
    let storage = storage_connect(dir.path()).await?;
    Ok((storage, dir))
}

/// Build a minimal `NewTrack` fixture for storage tests.
#[must_use]
pub fn make_track(title: &str, path: &Path, album_id: Option<i64>) -> NewTrack {
    NewTrack {
        title: title.to_string(),
        track_number: Some(1),
        disc_number: Some(1),
        duration: 180.0,
        audio: TrackAudio {
            file_path: path.to_string_lossy().to_string(),
            content_hash: None,
            format: "FLAC".to_string(),
            sample_rate: 44100,
            bit_depth: Some(16),
            channels: 2,
            codec: "flac".to_string(),
            lossless: true,
            bitrate: None,
            album_id,
            artist_id: None,
            file_size: 1024,
            last_modified: "2024-01-01T00:00:00Z".to_string(),
        },
    }
}

/// Build a minimal `NewAlbum` fixture for storage tests.
#[must_use]
pub fn make_album(title: &str, artist_id: i64, year: i32) -> NewAlbum {
    NewAlbum {
        title: title.to_string(),
        artist_id,
        year: Some(year),
        genre: Some("Rock".to_string()),
        format_summary: "FLAC 16-bit/44.1kHz".to_string(),
        lossless: true,
        format: "FLAC".to_string(),
        bit_depth: Some(16),
        sample_rate: Some(44100),
        artwork_path: None,
    }
}

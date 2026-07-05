//! Integration tests for the storage layer (`SqliteStorage` + `Storage` trait).

use std::path::Path;

use {
    anyhow::{Context, Result},
    tempfile::{TempDir, tempdir},
};

use oxhidifi_refactor::storage::{NewTrack, TrackAudio, database::SqliteStorage};

/// Create a temporary `SqliteStorage` instance for testing.
///
/// # Errors
///
/// Returns an error if the temp directory or database connection cannot be created.
async fn test_storage() -> Result<(SqliteStorage, TempDir)> {
    let dir = tempdir().context("failed to create temp dir")?;
    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::connect(&db_path)
        .await
        .context("failed to connect to storage")?;
    Ok((storage, dir))
}

fn make_track(title: &str, path: &Path, album_id: Option<i64>) -> NewTrack {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use {
        anyhow::{Context, Result, ensure},
        tokio::test,
    };

    use oxhidifi_refactor::storage::{
        NewAlbum, NewArtist, NewQueueEntry, QueueContext, Storage, TrackUpdate,
    };

    use crate::{make_track, test_storage};

    #[test]
    async fn insert_and_get_artist() -> Result<()> {
        let (storage, _dir) = test_storage().await?;
        let artist_id = storage
            .insert_artist(NewArtist {
                name: "Test Artist".to_string(),
            })
            .await?;
        let artist = storage
            .get_artist(artist_id)
            .await?
            .context("artist not found")?;

        ensure!(
            artist.name == "Test Artist",
            "unexpected artist name: {}",
            artist.name
        );
        Ok(())
    }

    #[test]
    async fn insert_and_get_album() -> anyhow::Result<()> {
        let (storage, _dir) = test_storage().await?;
        let artist_id = storage
            .insert_artist(NewArtist {
                name: "Album Artist".to_string(),
            })
            .await?;
        let album_id = storage
            .insert_album(NewAlbum {
                title: "Test Album".to_string(),
                artist_id,
                year: Some(2024),
                genre: Some("Rock".to_string()),
                artwork_path: None,
                format_summary: "FLAC 16-bit/44.1kHz".to_string(),
                lossless: true,
                format: "FLAC".to_string(),
                bit_depth: Some(16),
                sample_rate: Some(44100),
            })
            .await?;
        let album = storage
            .get_album(album_id)
            .await?
            .context("album not found")?;

        ensure!(
            album.title == "Test Album",
            "unexpected album title: {}",
            album.title
        );
        Ok(())
    }

    #[test]
    async fn insert_and_get_track() -> Result<()> {
        let (storage, _dir) = test_storage().await?;
        let track = make_track("Test Track", Path::new("/music/test.flac"), None);
        let track_id = storage.insert_track(track).await?;
        let fetched = storage
            .get_track(track_id)
            .await?
            .context("track not found")?;

        ensure!(
            fetched.title == "Test Track",
            "unexpected track title: {}",
            fetched.title
        );
        Ok(())
    }

    #[test]
    async fn track_crud() -> Result<()> {
        let (storage, _dir) = test_storage().await?;
        let track = make_track("CRUD Track", Path::new("/music/crud.flac"), None);
        let track_id = storage.insert_track(track).await?;
        let fetched = storage
            .get_track(track_id)
            .await?
            .context("track not found")?;
        ensure!(
            fetched.title == "CRUD Track",
            "unexpected title: {}",
            fetched.title
        );

        storage
            .update_track(
                track_id,
                TrackUpdate {
                    title: Some("Updated Track".to_string()),
                    ..TrackUpdate::default()
                },
            )
            .await?;

        let updated = storage
            .get_track(track_id)
            .await?
            .context("track not found after update")?;
        ensure!(
            updated.title == "Updated Track",
            "unexpected title after update: {}",
            updated.title
        );

        storage.delete_track(track_id).await?;

        ensure!(
            matches!(storage.get_track(track_id).await, Ok(None)),
            "track should have been deleted"
        );
        Ok(())
    }

    #[test]
    async fn duplicate_detection_by_path() -> Result<()> {
        let (storage, _dir) = test_storage().await?;
        let path = Path::new("/music/unique.flac");
        let track = make_track("Unique Path", path, None);
        storage.insert_track(track).await?;
        let found = storage
            .find_by_path(path)
            .await?
            .context("track not found by path")?;
        ensure!(
            found.audio.file_path == "/music/unique.flac",
            "unexpected file path: {}",
            found.audio.file_path
        );

        ensure!(
            matches!(
                storage.find_by_path(Path::new("/nonexistent.flac")).await,
                Ok(None)
            ),
            "nonexistent path should not be found"
        );
        Ok(())
    }

    #[test]
    async fn duplicate_detection_by_hash() -> Result<()> {
        let (storage, _dir) = test_storage().await?;
        let hash = "abcdef1234567890";

        let mut track1 = make_track("Track 1", Path::new("/music/track1.flac"), None);
        track1.audio.content_hash = Some(hash.to_string());
        storage.insert_track(track1).await?;

        let mut track2 = make_track("Track 2", Path::new("/music/track2.flac"), None);
        track2.audio.content_hash = Some(hash.to_string());
        storage.insert_track(track2).await?;

        let found = storage.find_by_hash(hash).await?;
        ensure!(found.len() == 2, "expected 2 tracks, got {}", found.len());
        Ok(())
    }

    #[test]
    async fn album_track_relationships() -> Result<()> {
        let (storage, _dir) = test_storage().await?;

        let artist_id = storage
            .insert_artist(NewArtist {
                name: "Rel Artist".to_string(),
            })
            .await?;

        let album_id = storage
            .insert_album(NewAlbum {
                title: "Rel Album".to_string(),
                artist_id,
                year: Some(2024),
                genre: Some("Jazz".to_string()),
                artwork_path: None,
                format_summary: "FLAC 24-bit/96kHz".to_string(),
                lossless: true,
                format: "FLAC".to_string(),
                bit_depth: Some(24),
                sample_rate: Some(96000),
            })
            .await?;

        storage
            .insert_track(make_track(
                "Track 1",
                Path::new("/music/r1.flac"),
                Some(album_id),
            ))
            .await?;
        storage
            .insert_track(make_track(
                "Track 2",
                Path::new("/music/r2.flac"),
                Some(album_id),
            ))
            .await?;

        let tracks = storage.get_tracks_by_album(album_id).await?;
        ensure!(tracks.len() == 2, "expected 2 tracks, got {}", tracks.len());

        let albums = storage.get_albums_by_artist(artist_id).await?;
        ensure!(albums.len() == 1, "expected 1 album, got {}", albums.len());
        Ok(())
    }

    #[test]
    async fn queue_operations() -> Result<()> {
        let (storage, _dir) = test_storage().await?;

        let track1_id = storage
            .insert_track(make_track("Q1", Path::new("/music/q1.flac"), None))
            .await?;
        let track2_id = storage
            .insert_track(make_track("Q2", Path::new("/music/q2.flac"), None))
            .await?;

        let queue = vec![
            NewQueueEntry {
                track_id: track1_id,
                position: 0,
                context_type: Some("manual".to_string()),
                context_id: None,
            },
            NewQueueEntry {
                track_id: track2_id,
                position: 1,
                context_type: Some("manual".to_string()),
                context_id: None,
            },
        ];

        storage.set_queue(&queue).await?;

        let entries = storage.get_queue().await?;
        ensure!(
            entries.len() == 2,
            "expected 2 queue entries, got {}",
            entries.len()
        );
        ensure!(
            entries[0].track_id == track1_id,
            "expected track_id {track1_id} in queue, got {}",
            entries[0].track_id
        );

        storage
            .append_queue(track1_id, Some(QueueContext::Manual))
            .await?;

        let entries = storage.get_queue().await?;
        ensure!(
            entries.len() == 3,
            "expected 3 queue entries, got {}",
            entries.len()
        );

        storage.clear_queue().await?;
        let entries = storage.get_queue().await?;
        ensure!(entries.is_empty(), "queue should be empty");
        Ok(())
    }

    #[test]
    async fn library_directories() -> Result<()> {
        let (storage, _dir) = test_storage().await?;

        storage.add_library_directory(Path::new("/music")).await?;
        storage.add_library_directory(Path::new("/audio")).await?;

        let dirs = storage.list_library_directories().await?;
        ensure!(dirs.len() == 2, "expected 2 dirs, got {}", dirs.len());

        storage.remove_library_directory(dirs[0].id).await?;

        let dirs = storage.list_library_directories().await?;
        ensure!(dirs.len() == 1, "expected 1 dir, got {}", dirs.len());
        Ok(())
    }

    #[test]
    async fn track_search() -> Result<()> {
        let (storage, _dir) = test_storage().await?;

        storage
            .insert_track(make_track(
                "Bohemian Rhapsody",
                Path::new("/music/queen.flac"),
                None,
            ))
            .await?;
        storage
            .insert_track(make_track(
                "Stairway to Heaven",
                Path::new("/music/ledzep.flac"),
                None,
            ))
            .await?;

        let results = storage.search_tracks("Bohemian").await?;
        ensure!(
            results.len() == 1,
            "expected 1 result, got {}",
            results.len()
        );
        ensure!(
            results[0].title == "Bohemian Rhapsody",
            "unexpected title: {}",
            results[0].title
        );
        Ok(())
    }

    #[test]
    async fn get_all_artists() -> Result<()> {
        let (storage, _dir) = test_storage().await?;

        storage
            .insert_artist(NewArtist {
                name: "Artist A".to_string(),
            })
            .await?;
        storage
            .insert_artist(NewArtist {
                name: "Artist B".to_string(),
            })
            .await?;

        let all = storage.get_all_artists().await?;
        ensure!(all.len() == 2, "expected 2 artists, got {}", all.len());
        Ok(())
    }
}

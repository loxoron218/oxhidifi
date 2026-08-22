//! Integration tests for the storage layer (`SqliteStorage` + `Storage` trait).

mod db_setup;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use {
        anyhow::{Context, Result, ensure},
        tokio::test,
    };

    use oxhidifi::storage::{
        Storage,
        catalog::{NewArtist, NewQueueEntry, QueueContext::Manual, TrackUpdate},
    };

    use crate::db_setup::{make_album, make_track, test_storage};

    #[test]
    async fn insert_and_get_artist() -> Result<()> {
        let (storage, dir) = test_storage().await?;
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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn insert_and_get_album() -> Result<()> {
        let (storage, dir) = test_storage().await?;
        let artist_id = storage
            .insert_artist(NewArtist {
                name: "Album Artist".to_string(),
            })
            .await?;
        let album_id = storage
            .insert_album(make_album("Test Album", artist_id, 2024))
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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn insert_and_get_track() -> Result<()> {
        let (storage, dir) = test_storage().await?;
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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn track_crud() -> Result<()> {
        let (storage, dir) = test_storage().await?;
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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn duplicate_detection_by_path() -> Result<()> {
        let (storage, dir) = test_storage().await?;
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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn duplicate_detection_by_hash() -> Result<()> {
        let (storage, dir) = test_storage().await?;
        let hash = "abcdef1234567890";

        let mut track1 = make_track("Track 1", Path::new("/music/track1.flac"), None);
        track1.audio.content_hash = Some(hash.to_string());
        storage.insert_track(track1).await?;

        let mut track2 = make_track("Track 2", Path::new("/music/track2.flac"), None);
        track2.audio.content_hash = Some(hash.to_string());
        storage.insert_track(track2).await?;

        let found = storage.find_by_hash(hash).await?;
        ensure!(found.len() == 2, "expected 2 tracks, got {}", found.len());
        drop(dir);
        Ok(())
    }

    #[test]
    async fn album_track_relationships() -> Result<()> {
        let (storage, dir) = test_storage().await?;

        let artist_id = storage
            .insert_artist(NewArtist {
                name: "Rel Artist".to_string(),
            })
            .await?;

        let album_id = storage
            .insert_album(make_album("Rel Album", artist_id, 2024))
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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn queue_operations() -> Result<()> {
        let (storage, dir) = test_storage().await?;

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
            entries
                .first()
                .is_some_and(|entry| entry.track_id == track1_id),
            "expected track_id {track1_id} in queue, got {:?}",
            entries.first().map(|entry| entry.track_id)
        );

        storage.append_queue(track1_id, Some(Manual)).await?;

        let entries = storage.get_queue().await?;
        ensure!(
            entries.len() == 3,
            "expected 3 queue entries, got {}",
            entries.len()
        );

        storage.clear_queue().await?;
        let entries = storage.get_queue().await?;
        ensure!(entries.is_empty(), "queue should be empty");
        drop(dir);
        Ok(())
    }

    #[test]
    async fn library_directories() -> Result<()> {
        let (storage, dir) = test_storage().await?;

        storage.add_library_directory(Path::new("/music")).await?;
        storage.add_library_directory(Path::new("/audio")).await?;

        let dirs = storage.list_library_directories().await?;
        ensure!(dirs.len() == 2, "expected 2 dirs, got {}", dirs.len());

        let directory = dirs.first().context("expected a library directory")?;
        storage.remove_library_directory(directory.id).await?;

        let dirs = storage.list_library_directories().await?;
        ensure!(dirs.len() == 1, "expected 1 dir, got {}", dirs.len());
        drop(dir);
        Ok(())
    }

    #[test]
    async fn track_search() -> Result<()> {
        let (storage, dir) = test_storage().await?;

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
            results
                .first()
                .is_some_and(|track| track.title == "Bohemian Rhapsody"),
            "unexpected title: {:?}",
            results.first().map(|track| &track.title)
        );
        drop(dir);
        Ok(())
    }

    #[test]
    async fn get_all_artists() -> Result<()> {
        let (storage, dir) = test_storage().await?;

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
        drop(dir);
        Ok(())
    }

    #[test]
    async fn settings_setter_round_trips_in_memory() -> Result<()> {
        let (storage, dir) = test_storage().await?;

        storage.set_volume(1.0).await?;
        ensure!((storage.get_settings_volume() - 1.0).abs() < f64::EPSILON);
        storage.set_volume(0.5).await?;
        ensure!((storage.get_settings_volume() - 0.5).abs() < f64::EPSILON);
        storage.set_volume(0.0).await?;
        ensure!((storage.get_settings_volume() - 0.0).abs() < f64::EPSILON);

        storage.set_gapless_enabled(false).await?;
        ensure!(!storage.get_gapless_enabled());
        storage.set_gapless_enabled(true).await?;
        ensure!(storage.get_gapless_enabled());

        drop(dir);
        Ok(())
    }
}
